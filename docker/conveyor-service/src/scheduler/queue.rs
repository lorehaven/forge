//! The run queue, which is the `runs` table.
//!
//! A run is its own queue entry rather than a message on a broker: the record
//! and the claim live in one row, so a run cannot be queued twice, a restart
//! loses nothing, and "what is running" is answerable with a select rather than
//! by asking a queue that has already forgotten.
//!
//! Claiming is `FOR UPDATE SKIP LOCKED`, which is what lets several replicas
//! share one queue without coordinating.

use crate::domain::{Artifact, Job, Run, Status, Trigger};
use crate::executors::{LogChunk, StepState};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error(
        "conveyor's queue needs Postgres; this service is running against an \
         in-memory database, where a queue would be lost on every restart"
    )]
    NotPostgres,

    #[error("no such repository: {0}")]
    UnknownRepo(String),

    #[error("no such run: {0}")]
    UnknownRun(String),

    #[error("unreadable row: {0}")]
    BadRow(String),

    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

impl QueueError {
    /// Whether this is the unique violation raised when a second worker claims
    /// a run for a repository that already has one going.
    fn is_repo_busy(&self) -> bool {
        let Self::Sql(sqlx::Error::Database(error)) = self else {
            return false;
        };
        error.code().as_deref() == Some("23505")
    }
}

/// The pool, or a clear refusal.
///
/// The estate allows an in-memory database for tests. A scheduler on top of one
/// would look like it worked and lose every queued run on restart, so this says
/// so rather than degrading quietly.
pub fn pool(db: &Db) -> Result<&Pool<Postgres>, QueueError> {
    match db {
        Db::Postgres(postgres) => Ok(postgres.pool()),
        Db::InMemory(_) => Err(QueueError::NotPostgres),
    }
}

pub fn schema() -> String {
    envmnt::get_or("DB_SCHEMA", "conveyor")
}

// ---------------------------------------------------------------------------
// Enqueueing
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct NewRun {
    pub repo_id: String,
    pub trigger: Trigger,
    pub git_ref: String,
    pub sha: String,
    pub message: Option<String>,
    /// Provider delivery id, when a webhook started this.
    pub delivery_id: Option<String>,
}

/// What enqueueing did.
#[derive(Clone, Debug)]
pub enum Enqueued {
    Created(Box<Run>),
    /// This delivery has already been seen. A provider retries a webhook it did
    /// not get a prompt answer for, and a second run of the same commit would
    /// double every side effect the first one had.
    AlreadySeen(Box<Run>),
}

impl Enqueued {
    pub fn run(&self) -> &Run {
        match self {
            Self::Created(run) | Self::AlreadySeen(run) => run,
        }
    }

    pub const fn is_new(&self) -> bool {
        matches!(self, Self::Created(_))
    }
}

pub async fn enqueue(db: &Db, new: &NewRun) -> Result<Enqueued, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let id = Uuid::new_v4().to_string();

    // `DO NOTHING` rather than an existence check first: two deliveries of the
    // same webhook can arrive at two replicas at once, and only the index can
    // settle which of them wins.
    let sql = format!(
        "INSERT INTO {schema}.runs \
         (id, repo_id, trigger, git_ref, sha, message, delivery_id, status) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'queued') \
         ON CONFLICT (delivery_id) WHERE delivery_id IS NOT NULL DO NOTHING \
         RETURNING {RUN_COLUMNS}"
    );

    let inserted = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&id)
        .bind(&new.repo_id)
        .bind(new.trigger.as_str())
        .bind(&new.git_ref)
        .bind(&new.sha)
        .bind(&new.message)
        .bind(&new.delivery_id)
        .fetch_optional(pool)
        .await?;

    if let Some(row) = inserted {
        return Ok(Enqueued::Created(Box::new(run_from_row(&row)?)));
    }

    // Nothing was inserted, so the delivery id is already there.
    let delivery_id = new.delivery_id.as_deref().ok_or_else(|| {
        QueueError::BadRow("insert returned no row and had no delivery id".into())
    })?;

    let existing = find_by_delivery(db, delivery_id)
        .await?
        .ok_or_else(|| QueueError::BadRow("conflicting delivery vanished".into()))?;

    Ok(Enqueued::AlreadySeen(Box::new(existing)))
}

pub async fn find_by_delivery(db: &Db, delivery_id: &str) -> Result<Option<Run>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {RUN_COLUMNS} FROM {schema}.runs WHERE delivery_id = $1");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(delivery_id)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(run_from_row).transpose()
}

// ---------------------------------------------------------------------------
// Claiming
// ---------------------------------------------------------------------------

/// Takes the oldest queued run whose repository is free, or nothing.
///
/// The `NOT EXISTS` clause keeps a worker from picking up a repository that is
/// already building; the partial unique index behind it makes that a guarantee
/// rather than a hope, because the check and the claim are not one atomic act.
/// A collision comes back as `Ok(None)` - there was nothing this worker could
/// take, which is exactly what an empty queue means to the caller.
pub async fn claim_next(db: &Db, worker: &str) -> Result<Option<Run>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();

    let sql = format!(
        "UPDATE {schema}.runs SET \
             status = 'running', \
             claimed_by = $1, \
             claimed_at = NOW(), \
             started_at = COALESCE(started_at, NOW()), \
             attempt = attempt + 1 \
         WHERE id = ( \
             SELECT candidate.id FROM {schema}.runs candidate \
             JOIN {schema}.repos repo ON repo.id = candidate.repo_id \
             WHERE candidate.status = 'queued' \
               AND repo.enabled \
               AND NOT EXISTS ( \
                   SELECT 1 FROM {schema}.runs busy \
                   WHERE busy.repo_id = candidate.repo_id \
                     AND busy.status = 'running' \
               ) \
             ORDER BY candidate.queued_at \
             FOR UPDATE OF candidate SKIP LOCKED \
             LIMIT 1 \
         ) \
         RETURNING {RUN_COLUMNS}"
    );

    let claimed = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(worker)
        .fetch_optional(pool)
        .await;

    match claimed {
        Ok(Some(row)) => Ok(Some(run_from_row(&row)?)),
        Ok(None) => Ok(None),
        Err(error) => {
            let error = QueueError::from(error);
            if error.is_repo_busy() {
                tracing::debug!("another worker claimed this repository first");
                return Ok(None);
            }
            Err(error)
        }
    }
}

/// Says the worker holding this run is still alive.
pub async fn heartbeat(db: &Db, run_id: &str, worker: &str) -> Result<(), QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.runs SET claimed_at = NOW() \
         WHERE id = $1 AND claimed_by = $2 AND status = 'running'"
    );

    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(run_id)
        .bind(worker)
        .execute(pool)
        .await?;
    Ok(())
}

/// Puts back any run whose worker stopped saying it was alive.
///
/// A worker that was killed leaves its run `running` for ever otherwise, and
/// the partial unique index would keep that repository from ever building
/// again - one dead worker would take a repository out of service permanently.
pub async fn requeue_stale(db: &Db, stale_after_secs: u64) -> Result<u64, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.runs SET \
             status = 'queued', claimed_by = NULL, claimed_at = NULL \
         WHERE status = 'running' \
           AND claimed_at IS NOT NULL \
           AND claimed_at < NOW() - make_interval(secs => $1)"
    );

    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(stale_after_secs as f64)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

/// Asks for a run to stop. Whichever replica is holding it notices on its next
/// poll; a run that has not started yet is cancelled outright.
pub async fn request_cancel(db: &Db, run_id: &str) -> Result<bool, QueueError> {
    let pool = pool(db)?;
    let schema = schema();

    let sql = format!(
        "UPDATE {schema}.runs SET \
             cancel_requested = TRUE, \
             status = CASE WHEN status = 'queued' THEN 'cancelled' ELSE status END, \
             finished_at = CASE WHEN status = 'queued' THEN NOW() ELSE finished_at END \
         WHERE id = $1 AND status IN ('queued', 'running')"
    );

    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(run_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn is_cancel_requested(db: &Db, run_id: &str) -> Result<bool, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT cancel_requested FROM {schema}.runs WHERE id = $1");

    let requested: Option<(bool,)> = sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(run_id)
        .fetch_optional(pool)
        .await?;

    Ok(requested.is_some_and(|(flag,)| flag))
}

// ---------------------------------------------------------------------------
// Finishing
// ---------------------------------------------------------------------------

pub async fn finish_run(
    db: &Db,
    run_id: &str,
    status: Status,
    error: Option<&str>,
) -> Result<(), QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.runs SET \
             status = $2, error = $3, finished_at = NOW(), claimed_by = NULL \
         WHERE id = $1"
    );

    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(run_id)
        .bind(status.as_str())
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Jobs and steps
// ---------------------------------------------------------------------------

/// A job as the plan describes it, before it has run.
#[derive(Clone, Debug)]
pub struct PlannedJob {
    pub stage: String,
    pub name: String,
    pub needs: Vec<String>,
    /// `Queued` for a job that will run, `Skipped` for one the plan excluded.
    pub status: Status,
    /// Why it was excluded, when it was.
    pub error: Option<String>,
}

/// Writes the whole plan up front, skipped jobs included.
///
/// The alternative - inserting a job when it starts - leaves a run whose page
/// grows as it goes and which can never show what it decided not to do.
pub async fn create_jobs(
    db: &Db,
    run_id: &str,
    planned: &[PlannedJob],
) -> Result<Vec<Job>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "INSERT INTO {schema}.jobs (id, run_id, stage, name, needs, status, error) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {JOB_COLUMNS}"
    );

    let mut jobs = Vec::with_capacity(planned.len());
    for job in planned {
        let needs = serde_json::to_value(&job.needs)
            .map_err(|error| QueueError::BadRow(format!("needs is not serialisable: {error}")))?;

        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(Uuid::new_v4().to_string())
            .bind(run_id)
            .bind(&job.stage)
            .bind(&job.name)
            .bind(&needs)
            .bind(job.status.as_str())
            .bind(&job.error)
            .fetch_one(pool)
            .await?;

        jobs.push(job_from_row(&row)?);
    }

    Ok(jobs)
}

pub async fn start_job(db: &Db, job_id: &str) -> Result<(), QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql =
        format!("UPDATE {schema}.jobs SET status = 'running', started_at = NOW() WHERE id = $1");

    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(job_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn finish_job(
    db: &Db,
    job_id: &str,
    status: Status,
    exit_code: Option<i32>,
    error: Option<&str>,
) -> Result<(), QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.jobs SET \
             status = $2, exit_code = $3, error = $4, finished_at = NOW() \
         WHERE id = $1"
    );

    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(job_id)
        .bind(status.as_str())
        .bind(exit_code)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

/// Replaces this job's step rows with what the executor reported.
pub async fn record_steps(db: &Db, job_id: &str, steps: &[StepState]) -> Result<(), QueueError> {
    let pool = pool(db)?;
    let schema = schema();

    // Written once, at the end, so a retried job does not accumulate two sets
    // of rows under the same ordinals.
    let clear = format!("DELETE FROM {schema}.steps WHERE job_id = $1");
    sqlx::query(sqlx::AssertSqlSafe(clear.as_str()))
        .bind(job_id)
        .execute(pool)
        .await?;

    let sql = format!(
        "INSERT INTO {schema}.steps \
         (id, job_id, ordinal, kind, command, status, exit_code, started_at, finished_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    );

    for step in steps {
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(Uuid::new_v4().to_string())
            .bind(job_id)
            .bind(i32::try_from(step.ordinal).unwrap_or(i32::MAX))
            .bind(&step.kind)
            .bind(&step.command)
            .bind(step.status.as_str())
            .bind(step.exit_code)
            .bind(step.started_at)
            .bind(step.finished_at)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Appends output for a job.
///
/// Persisted when the job finishes rather than line by line: a running job's
/// output is served live from the executor, and writing every line as it
/// arrives would put a database round trip in the path of a build's stdout.
/// The cost is that a worker killed mid-job loses that job's log.
pub async fn append_logs(db: &Db, job_id: &str, chunks: &[LogChunk]) -> Result<(), QueueError> {
    if chunks.is_empty() {
        return Ok(());
    }

    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "INSERT INTO {schema}.logs (job_id, seq, stream, written_at, chunk) \
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (job_id, seq) DO NOTHING"
    );

    let mut transaction = pool.begin().await?;
    for chunk in chunks {
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(job_id)
            .bind(i64::try_from(chunk.seq).unwrap_or(i64::MAX))
            .bind(chunk.stream.as_str())
            .bind(chunk.at)
            .bind(&chunk.line)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;

    Ok(())
}

/// Records something a job produced and left somewhere durable.
pub async fn record_artifact(db: &Db, artifact: &Artifact) -> Result<(), QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "INSERT INTO {schema}.artifacts \
         (id, run_id, job_id, kind, name, version, uri, digest) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
    );

    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&artifact.id)
        .bind(&artifact.run_id)
        .bind(&artifact.job_id)
        .bind(&artifact.kind)
        .bind(&artifact.name)
        .bind(&artifact.version)
        .bind(&artifact.uri)
        .bind(&artifact.digest)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_artifacts(db: &Db, run_id: &str) -> Result<Vec<Artifact>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT id, run_id, job_id, kind, name, version, uri, digest, created_at \
         FROM {schema}.artifacts WHERE run_id = $1 ORDER BY created_at, name"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(run_id)
        .fetch_all(pool)
        .await?;

    rows.iter()
        .map(|row| {
            Ok(Artifact {
                id: row.try_get("id")?,
                run_id: row.try_get("run_id")?,
                job_id: row.try_get("job_id")?,
                kind: row.try_get("kind")?,
                name: row.try_get("name")?,
                version: row.try_get("version")?,
                uri: row.try_get("uri")?,
                digest: row.try_get("digest")?,
                created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

pub async fn read_run(db: &Db, run_id: &str) -> Result<Option<Run>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {RUN_COLUMNS} FROM {schema}.runs WHERE id = $1");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(run_id)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(run_from_row).transpose()
}

pub async fn list_runs(db: &Db, repo_id: Option<&str>, limit: i64) -> Result<Vec<Run>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT {RUN_COLUMNS} FROM {schema}.runs \
         WHERE ($1::text IS NULL OR repo_id = $1) \
         ORDER BY queued_at DESC LIMIT $2"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(repo_id)
        .bind(limit.clamp(1, 500))
        .fetch_all(pool)
        .await?;

    rows.iter().map(run_from_row).collect()
}

pub async fn list_jobs(db: &Db, run_id: &str) -> Result<Vec<Job>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT {JOB_COLUMNS} FROM {schema}.jobs WHERE run_id = $1 \
         ORDER BY started_at NULLS LAST, stage, name"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(run_id)
        .fetch_all(pool)
        .await?;

    rows.iter().map(job_from_row).collect()
}

/// Output for a finished job, from `after` onwards.
pub async fn read_logs(db: &Db, job_id: &str, after: i64) -> Result<Vec<LogChunk>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT seq, stream, written_at, chunk FROM {schema}.logs \
         WHERE job_id = $1 AND seq > $2 ORDER BY seq"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(job_id)
        .bind(after)
        .fetch_all(pool)
        .await?;

    rows.iter()
        .map(|row| {
            let stream: String = row.try_get("stream")?;
            Ok(LogChunk {
                seq: u64::try_from(row.try_get::<i64, _>("seq")?).unwrap_or(0),
                stream: crate::executors::Stream::parse(&stream)
                    .ok_or_else(|| QueueError::BadRow(format!("unknown stream '{stream}'")))?,
                line: row.try_get("chunk")?,
                at: row.try_get::<DateTime<Utc>, _>("written_at")?,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

const RUN_COLUMNS: &str = "id, repo_id, trigger, git_ref, sha, message, delivery_id, status, \
                           queued_at, started_at, finished_at, claimed_by, claimed_at, attempt, \
                           error";

const JOB_COLUMNS: &str = "id, run_id, stage, name, needs, status, exit_code, started_at, \
                           finished_at, error";

fn run_from_row(row: &sqlx::postgres::PgRow) -> Result<Run, QueueError> {
    let trigger: String = row.try_get("trigger")?;
    let status: String = row.try_get("status")?;

    Ok(Run {
        id: row.try_get("id")?,
        repo_id: row.try_get("repo_id")?,
        trigger: Trigger::parse(&trigger)
            .ok_or_else(|| QueueError::BadRow(format!("unknown trigger '{trigger}'")))?,
        git_ref: row.try_get("git_ref")?,
        sha: row.try_get("sha")?,
        message: row.try_get("message")?,
        delivery_id: row.try_get("delivery_id")?,
        status: Status::parse(&status)
            .ok_or_else(|| QueueError::BadRow(format!("unknown status '{status}'")))?,
        queued_at: row.try_get::<DateTime<Utc>, _>("queued_at")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        claimed_by: row.try_get("claimed_by")?,
        claimed_at: row.try_get("claimed_at")?,
        attempt: row.try_get("attempt")?,
        error: row.try_get("error")?,
    })
}

fn job_from_row(row: &sqlx::postgres::PgRow) -> Result<Job, QueueError> {
    let status: String = row.try_get("status")?;
    let needs: serde_json::Value = row.try_get("needs")?;

    Ok(Job {
        id: row.try_get("id")?,
        run_id: row.try_get("run_id")?,
        stage: row.try_get("stage")?,
        name: row.try_get("name")?,
        needs: serde_json::from_value(needs)
            .map_err(|error| QueueError::BadRow(format!("needs is not a string list: {error}")))?,
        status: Status::parse(&status)
            .ok_or_else(|| QueueError::BadRow(format!("unknown status '{status}'")))?,
        exit_code: row.try_get("exit_code")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        error: row.try_get("error")?,
    })
}
