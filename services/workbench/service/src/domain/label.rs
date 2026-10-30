//! Labels, scoped to a project, and the issue↔label join.

use crate::domain::db::{WorkbenchError, pool, schema};
use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_db::prelude::Db;
use quench_serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Label {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub color: String,
    pub created_at: Time,
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

    let row = pool
        .query_one(&sql, &[&id, &new.project_id, &new.name, &new.color])
        .await?;

    from_row(&row)
}

/// Used before delete/attach/detach to resolve the project id for auth.
pub async fn read(db: &Db, id: &str) -> Result<Option<Label>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.labels WHERE id = $1");

    let row = pool.query_opt(&sql, &[&id]).await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn list_by_project(db: &Db, project_id: &str) -> Result<Vec<Label>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.labels WHERE project_id = $1 ORDER BY name");

    let rows = pool.query(&sql, &[&project_id]).await?;

    rows.iter().map(from_row).collect()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.labels WHERE id = $1");

    let result = pool.execute(&sql, &[&id]).await?;

    Ok(result > 0)
}

/// Attaches `label_id` to `issue_id`; idempotent, not a duplicate-key error.
pub async fn attach(db: &Db, issue_id: &str, label_id: &str) -> Result<(), WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "INSERT INTO {schema}.issue_labels (issue_id, label_id) \
         VALUES ($1, $2) ON CONFLICT DO NOTHING"
    );

    pool.execute(&sql, &[&issue_id, &label_id]).await?;

    Ok(())
}

pub async fn detach(db: &Db, issue_id: &str, label_id: &str) -> Result<(), WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.issue_labels WHERE issue_id = $1 AND label_id = $2");

    pool.execute(&sql, &[&issue_id, &label_id]).await?;

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

    let rows = pool.query(&sql, &[&issue_id]).await?;

    rows.iter().map(from_row).collect()
}

const COLUMNS: &str = "id, project_id, name, color, created_at";

fn from_row(row: &quench_pg::Row) -> Result<Label, WorkbenchError> {
    Ok(Label {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        name: row.get("name")?,
        color: row.get("color")?,
        created_at: row.get::<Time, _>("created_at")?,
    })
}
