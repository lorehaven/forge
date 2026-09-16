//! Issues, and the transactional `seq` assignment their key display depends on.

use crate::domain::db::{WorkbenchError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use sqlx::Row;
use uuid::Uuid;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The fixed v1 workflow (no per-project config yet). `blocked`/`rejected`
/// bracket the three original states as the "not moving forward" outliers.
pub const STATUSES: [&str; 5] = ["blocked", "todo", "in-progress", "done", "rejected"];

impl Issue {
    /// `WB-3`; not stored - built from `seq` to avoid a join on every read.
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

    let mut tx = pool.begin().await?;

    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&new.project_id)
        .execute(&mut *tx)
        .await?;

    let seq_sql = format!(
        "SELECT COALESCE(MAX(seq), 0) + 1 AS next_seq FROM {schema}.issues WHERE project_id = $1"
    );
    let seq: i32 = sqlx::query(sqlx::AssertSqlSafe(seq_sql.as_str()))
        .bind(&new.project_id)
        .fetch_one(&mut *tx)
        .await?
        .try_get("next_seq")?;

    let insert_sql = format!(
        "INSERT INTO {schema}.issues \
         (id, project_id, parent_id, seq, kind, title, description, priority, assignee, reporter, estimate) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         RETURNING {COLUMNS}"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind(&id)
        .bind(&new.project_id)
        .bind(&new.parent_id)
        .bind(seq)
        .bind(&new.kind)
        .bind(&new.title)
        .bind(&new.description)
        .bind(&new.priority)
        .bind(&new.assignee)
        .bind(&new.reporter)
        .bind(new.estimate)
        .fetch_one(&mut *tx)
        .await?;

    let issue = from_row(&row)?;
    tx.commit().await?;

    Ok(issue)
}

pub async fn read(db: &Db, id: &str) -> Result<Option<Issue>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.issues WHERE id = $1");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?;

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

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(project_id)
        .bind(seq)
        .fetch_optional(pool)
        .await?;

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

    let rows = match status {
        Some(status) => {
            let sql = format!(
                "SELECT {COLUMNS} FROM {schema}.issues \
                 WHERE project_id = $1 AND status = $2 ORDER BY seq"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(project_id)
                .bind(status)
                .fetch_all(pool)
                .await?
        }
        None => {
            let sql =
                format!("SELECT {COLUMNS} FROM {schema}.issues WHERE project_id = $1 ORDER BY seq");
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(project_id)
                .fetch_all(pool)
                .await?
        }
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

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(&changes.title)
        .bind(&changes.description)
        .bind(&changes.kind)
        .bind(&changes.priority)
        .bind(&changes.assignee)
        .bind(changes.estimate)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// Whether `status` is one of the fixed v1 workflow's states.
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

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(status)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.issues WHERE id = $1");

    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

const COLUMNS: &str = "id, project_id, parent_id, seq, kind, title, description, \
                       status, priority, assignee, reporter, estimate, created_at, updated_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Issue, WorkbenchError> {
    Ok(Issue {
        id: row.try_get("id")?,
        project_id: row.try_get("project_id")?,
        parent_id: row.try_get("parent_id")?,
        seq: row.try_get("seq")?,
        kind: row.try_get("kind")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        status: row.try_get("status")?,
        priority: row.try_get("priority")?,
        assignee: row.try_get("assignee")?,
        reporter: row.try_get("reporter")?,
        estimate: row.try_get("estimate")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}
