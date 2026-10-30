//! Running pipelines on a clock.
//!
//! A repository's `.conveyor.toml` declares `on.schedule`; this module keeps a row per declared
//! schedule ([`sync`], called when the repository's default branch is built) and starts a `schedule`
//! run when one comes due ([`spawn`]).
//!
//! Delivery is at-least-once and made idempotent by the run's `delivery_id`
//! (`schedule:<id>:<due instant>`): the run is enqueued *before* the schedule advances, so a crash
//! between the two re-enqueues the same firing and finds it already there, and two replicas racing
//! the same firing enqueue one run. A service that was down over a firing starts one catch-up run,
//! not one per missed night.

use crate::domain::{Repo, Trigger};
use crate::routers::api::runs::{TriggerError, trigger_run};
use crate::scheduler::queue::{QueueError, pool, schema};
use crate::scheduler::repos;
use conveyor_pipeline::Schedule;
use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_db::prelude::Db;
use quench_serde::Serialize;
use std::time::Duration;

/// How often the loop looks for due schedules. A firing starts at most this late.
const TICK: Duration = Duration::from_secs(15);

/// A stored schedule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StoredSchedule {
    pub id: String,
    pub repo_id: String,
    pub expression: String,
    pub timezone: String,
    pub next_run_at: Time,
    pub last_run_at: Option<Time>,
}

impl StoredSchedule {
    fn as_schedule(&self) -> Schedule {
        Schedule {
            expression: self.expression.clone(),
            timezone: self.timezone.clone(),
        }
    }
}

const COLUMNS: &str = "id, repo_id, expression, timezone, next_run_at, last_run_at";

fn from_row(row: &quench_pg::Row) -> Result<StoredSchedule, QueueError> {
    Ok(StoredSchedule {
        id: row.get("id")?,
        repo_id: row.get("repo_id")?,
        expression: row.get("expression")?,
        timezone: row.get("timezone")?,
        next_run_at: row.get::<Time, _>("next_run_at")?,
        last_run_at: row.get("last_run_at")?,
    })
}

/// A repository's schedules, soonest first.
pub async fn list(db: &Db, repo_id: &str) -> Result<Vec<StoredSchedule>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT {COLUMNS} FROM {schema}.schedules WHERE repo_id = $1 ORDER BY next_run_at, expression"
    );
    let rows = pool
        .query(&sql, &[&repo_id as &dyn quench_pg::ToSql])
        .await?;
    rows.iter().map(from_row).collect()
}

/// Makes the stored schedules of `repo_id` exactly `declared`.
///
/// New ones are inserted (first due at their next instant after `now`), ones no longer declared are
/// removed, ones still declared are left alone so their place in time is not reset by every push.
/// Returns how many were added and removed.
pub async fn sync(
    db: &Db,
    repo_id: &str,
    declared: &[Schedule],
    now: Time,
) -> Result<(usize, usize), QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let existing = list(db, repo_id).await?;

    let mut removed = 0;
    for stored in &existing {
        if !declared
            .iter()
            .any(|d| d.expression == stored.expression && d.timezone == stored.timezone)
        {
            pool.execute(
                &format!("DELETE FROM {schema}.schedules WHERE id = $1"),
                &[&stored.id as &dyn quench_pg::ToSql],
            )
            .await?;
            removed += 1;
        }
    }

    let mut added = 0;
    for schedule in declared {
        let known = existing
            .iter()
            .any(|s| s.expression == schedule.expression && s.timezone == schedule.timezone);
        if known {
            continue;
        }
        let Some(next) = schedule.next_after(now) else {
            continue;
        };
        let id = Uuid::new_v4().to_string();
        let inserted = pool
            .execute(
                &format!(
                    "INSERT INTO {schema}.schedules (id, repo_id, expression, timezone, next_run_at) \
                     VALUES ($1, $2, $3, $4, $5) \
                     ON CONFLICT (repo_id, expression, timezone) DO NOTHING"
                ),
                &[
                    &id as &dyn quench_pg::ToSql,
                    &repo_id,
                    &schedule.expression,
                    &schedule.timezone,
                    &next,
                ],
            )
            .await?;
        added += usize::try_from(inserted).unwrap_or(0);
    }
    Ok((added, removed))
}

/// Schedules whose firing instant has passed.
async fn due(db: &Db, now: Time) -> Result<Vec<StoredSchedule>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT {COLUMNS} FROM {schema}.schedules WHERE next_run_at <= $1 ORDER BY next_run_at"
    );
    let rows = pool.query(&sql, &[&now as &dyn quench_pg::ToSql]).await?;
    rows.iter().map(from_row).collect()
}

/// Moves a schedule past the firing it just produced. Compare-and-set on the old instant: if another
/// replica already advanced it, nothing changes.
async fn advance(db: &Db, stored: &StoredSchedule, next: Time) -> Result<bool, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let changed = pool
        .execute(
            &format!(
                "UPDATE {schema}.schedules \
                 SET next_run_at = $3, last_run_at = $2, updated_at = NOW() \
                 WHERE id = $1 AND next_run_at = $2"
            ),
            &[
                &stored.id as &dyn quench_pg::ToSql,
                &stored.next_run_at,
                &next,
            ],
        )
        .await?;
    Ok(changed > 0)
}

/// The identity of one firing: the same schedule due at the same instant is always the same run.
#[must_use]
pub fn delivery_id(schedule_id: &str, due_at: Time) -> String {
    format!("schedule:{schedule_id}:{}", due_at.timestamp())
}

/// What one pass did, for logs and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tick {
    pub started: usize,
    /// Firings that were already enqueued (another replica, or this one before a crash).
    pub duplicates: usize,
    /// Firings dropped: repository gone or disabled.
    pub skipped: usize,
    /// Firings left due because the repository could not be reached; retried next tick.
    pub retried: usize,
}

/// Starts a run for every schedule that has come due as of `now`, and moves each past it.
pub async fn fire_due(db: &Db, now: Time) -> Result<Tick, QueueError> {
    let mut tick = Tick::default();
    for stored in due(db, now).await? {
        let Some(repo) = repos::read(db, &stored.repo_id).await? else {
            tick.skipped += 1;
            continue;
        };
        // Next firing counted from *now*, not from the missed instant: a long outage yields one
        // catch-up run, not a burst.
        let next = stored.as_schedule().next_after(now);

        match start(db, &repo, &stored).await {
            Ok(created) => {
                if created {
                    tick.started += 1;
                    quench_log::info!(
                        "schedule `{}` ({}) of {} came due: run started",
                        stored.expression,
                        stored.timezone,
                        repo.slug()
                    );
                } else {
                    tick.duplicates += 1;
                }
            }
            Err(TriggerError::Disabled | TriggerError::NotFound) => tick.skipped += 1,
            Err(TriggerError::Queue(error)) => return Err(error),
            Err(error) => {
                // Probably the remote is unreachable: leave it due and try again next tick.
                quench_log::warn!(
                    "schedule `{}` of {} is due but could not start: {error}",
                    stored.expression,
                    repo.slug()
                );
                tick.retried += 1;
                continue;
            }
        }

        if let Some(next) = next {
            advance(db, &stored, next).await?;
        } else {
            // Cannot happen for a validated expression; do not loop on it forever.
            quench_log::error!(
                "schedule `{}` has no next firing; removing it",
                stored.expression
            );
            let pool = pool(db)?;
            pool.execute(
                &format!("DELETE FROM {}.schedules WHERE id = $1", schema()),
                &[&stored.id as &dyn quench_pg::ToSql],
            )
            .await?;
        }
    }
    Ok(tick)
}

/// Enqueues the run for one firing; `false` if that firing already had one.
async fn start(db: &Db, repo: &Repo, stored: &StoredSchedule) -> Result<bool, TriggerError> {
    let enqueued = trigger_run(
        db,
        &repo.id,
        None,
        None,
        Trigger::Schedule,
        Some(delivery_id(&stored.id, stored.next_run_at)),
        None,
    )
    .await?;
    Ok(enqueued.is_new())
}

/// Runs [`fire_due`] every few seconds for the life of the process.
pub fn spawn(db: Db) {
    if let Err(error) = pool(&db) {
        quench_log::error!("schedules not started: {error}");
        return;
    }
    tokio::spawn(async move {
        loop {
            if let Err(error) = fire_due(&db, Time::now()).await {
                quench_log::error!("could not check schedules: {error}");
            }
            tokio::time::sleep(TICK).await;
        }
    });
}
