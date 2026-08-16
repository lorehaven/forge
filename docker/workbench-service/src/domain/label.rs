//! Labels, scoped to a project, and the issue↔label join.

use crate::domain::db::{WorkbenchError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Label {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub color: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct NewLabel {
    pub project_id: String,
    pub name: String,
    pub color: String,
}

pub async fn create(db: &Db, new: &NewLabel) -> Result<Label, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let id = Uuid::new_v4().to_string();

    let sql = format!(
        "INSERT INTO {schema}.labels (id, project_id, name, color) \
         VALUES ($1, $2, $3, $4) RETURNING {COLUMNS}"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&id)
        .bind(&new.project_id)
        .bind(&new.name)
        .bind(&new.color)
        .fetch_one(pool)
        .await?;

    from_row(&row)
}

/// Looked up before a delete or an attach/detach, to resolve back to the
/// project an authorization check needs.
pub async fn read(db: &Db, id: &str) -> Result<Option<Label>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.labels WHERE id = $1");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn list_by_project(db: &Db, project_id: &str) -> Result<Vec<Label>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.labels WHERE project_id = $1 ORDER BY name");

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(project_id)
        .fetch_all(pool)
        .await?;

    rows.iter().map(from_row).collect()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.labels WHERE id = $1");

    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Attaches `label_id` to `issue_id`. Idempotent - applying a label a second
/// time is a no-op, not a duplicate-key error a caller has to catch.
pub async fn attach(db: &Db, issue_id: &str, label_id: &str) -> Result<(), WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "INSERT INTO {schema}.issue_labels (issue_id, label_id) \
         VALUES ($1, $2) ON CONFLICT DO NOTHING"
    );

    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(issue_id)
        .bind(label_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn detach(db: &Db, issue_id: &str, label_id: &str) -> Result<(), WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.issue_labels WHERE issue_id = $1 AND label_id = $2");

    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(issue_id)
        .bind(label_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// The labels attached to one issue.
pub async fn list_for_issue(db: &Db, issue_id: &str) -> Result<Vec<Label>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT l.id, l.project_id, l.name, l.color, l.created_at \
         FROM {schema}.labels l \
         JOIN {schema}.issue_labels il ON il.label_id = l.id \
         WHERE il.issue_id = $1 ORDER BY l.name"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(issue_id)
        .fetch_all(pool)
        .await?;

    rows.iter().map(from_row).collect()
}

const COLUMNS: &str = "id, project_id, name, color, created_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Label, WorkbenchError> {
    Ok(Label {
        id: row.try_get("id")?,
        project_id: row.try_get("project_id")?,
        name: row.try_get("name")?,
        color: row.try_get("color")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
    })
}
