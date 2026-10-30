//! Issues, and the transactional `seq` assignment their key display depends on.

use crate::domain::db::{WorkbenchError, pool, schema};
use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_db::prelude::Db;

#[derive(Clone, Debug, quench_serde::Serialize, quench_serde::Deserialize)]
pub struct Issue {
    pub id: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub seq: i32,
    pub kind: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: String,
    pub assignee: Option<String>,
    pub reporter: String,
    /// Story points; nullable since not every issue is sized yet.
    pub estimate: Option<i32>,
    pub created_at: Time,
    pub updated_at: Time,
}

/// The fixed v1 workflow (no per-project config yet). `blocked`/`rejected`
/// bracket the three original states as the "not moving forward" outliers.
pub const STATUSES: [&str; 5] = ["blocked", "todo", "in-progress", "done", "rejected"];

impl Issue {
    /// `WB-3`; not stored - built from `seq` to avoid a join on every read.
    #[must_use]
    pub fn key(&self, project_key: &str) -> String {
        format!("{project_key}-{}", self.seq)
    }
}

#[derive(Clone, Debug)]
pub struct NewIssue {
    pub project_id: String,
    pub parent_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: String,
    pub assignee: Option<String>,
    pub reporter: String,
    pub estimate: Option<i32>,
}

/// Assigns `seq` = `MAX(seq) + 1` and inserts, in one transaction; a
/// `pg_advisory_xact_lock` on the project id serializes concurrent creates.
pub async fn create(db: &Db, new: &NewIssue) -> Result<Issue, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let id = Uuid::new_v4().to_string();

    let mut conn = pool.get().await?;
    let mut tx = conn.begin().await?;

    tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&new.project_id],
    )
    .await?;

    let seq_sql = format!(
        "SELECT COALESCE(MAX(seq), 0) + 1 AS next_seq FROM {schema}.issues WHERE project_id = $1"
    );
    let seq: i32 = tx
        .query_one(&seq_sql, &[&new.project_id])
        .await?
        .get("next_seq")?;

    let insert_sql = format!(
        "INSERT INTO {schema}.issues \
         (id, project_id, parent_id, seq, kind, title, description, priority, assignee, reporter, estimate) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         RETURNING {COLUMNS}"
    );
    let row = tx
        .query_one(
            &insert_sql,
            &[
                &id,
                &new.project_id,
                &new.parent_id,
                &seq,
                &new.kind,
                &new.title,
                &new.description,
                &new.priority,
                &new.assignee,
                &new.reporter,
                &new.estimate,
            ],
        )
        .await?;

    let issue = from_row(&row)?;
    tx.commit().await?;
    drop(conn);

    Ok(issue)
}

pub async fn read(db: &Db, id: &str) -> Result<Option<Issue>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.issues WHERE id = $1");

    let row = pool.query_opt(&sql, &[&id]).await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn read_by_seq(
    db: &Db,
    project_id: &str,
    seq: i32,
) -> Result<Option<Issue>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.issues WHERE project_id = $1 AND seq = $2");

    let row = pool.query_opt(&sql, &[&project_id, &seq]).await?;

    row.as_ref().map(from_row).transpose()
}

/// Every issue in a project, optionally narrowed to one status (board view).
pub async fn list_by_project(
    db: &Db,
    project_id: &str,
    status: Option<&str>,
) -> Result<Vec<Issue>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();

    let rows = if let Some(status) = status {
        let sql = format!(
            "SELECT {COLUMNS} FROM {schema}.issues \
             WHERE project_id = $1 AND status = $2 ORDER BY seq"
        );
        pool.query(&sql, &[&project_id, &status]).await?
    } else {
        let sql =
            format!("SELECT {COLUMNS} FROM {schema}.issues WHERE project_id = $1 ORDER BY seq");
        pool.query(&sql, &[&project_id]).await?
    };

    rows.iter().map(from_row).collect()
}

#[derive(Clone, Debug)]
pub struct IssueUpdate {
    pub title: String,
    pub description: Option<String>,
    pub kind: String,
    pub priority: String,
    pub assignee: Option<String>,
    pub estimate: Option<i32>,
}

pub async fn update(
    db: &Db,
    id: &str,
    changes: &IssueUpdate,
) -> Result<Option<Issue>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.issues SET title = $2, description = $3, kind = $4, \
         priority = $5, assignee = $6, estimate = $7, updated_at = NOW() \
         WHERE id = $1 RETURNING {COLUMNS}"
    );

    let row = pool
        .query_opt(
            &sql,
            &[
                &id,
                &changes.title,
                &changes.description,
                &changes.kind,
                &changes.priority,
                &changes.assignee,
                &changes.estimate,
            ],
        )
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// Whether `status` is one of the fixed v1 workflow's states.
#[must_use]
pub fn is_valid_status(status: &str) -> bool {
    STATUSES.contains(&status)
}

pub async fn transition(db: &Db, id: &str, status: &str) -> Result<Option<Issue>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.issues SET status = $2, updated_at = NOW() \
         WHERE id = $1 RETURNING {COLUMNS}"
    );

    let row = pool.query_opt(&sql, &[&id, &status]).await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.issues WHERE id = $1");

    let result = pool.execute(&sql, &[&id]).await?;

    Ok(result > 0)
}

const COLUMNS: &str = "id, project_id, parent_id, seq, kind, title, description, \
                       status, priority, assignee, reporter, estimate, created_at, updated_at";

fn from_row(row: &quench_pg::Row) -> Result<Issue, WorkbenchError> {
    Ok(Issue {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        parent_id: row.get("parent_id")?,
        seq: row.get("seq")?,
        kind: row.get("kind")?,
        title: row.get("title")?,
        description: row.get("description")?,
        status: row.get("status")?,
        priority: row.get("priority")?,
        assignee: row.get("assignee")?,
        reporter: row.get("reporter")?,
        estimate: row.get("estimate")?,
        created_at: row.get::<Time, _>("created_at")?,
        updated_at: row.get::<Time, _>("updated_at")?,
    })
}
