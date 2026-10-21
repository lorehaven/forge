use crate::domain::db::{DaybookError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct Task {
    pub id: String,
    pub project_id: Option<String>,
    pub content: String,
    pub description: Option<String>,
    pub is_completed: bool,
    pub sort_order: i32,
    pub due_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct NewTask {
    pub content: String,
    pub description: Option<String>,
    pub project_id: Option<String>,
    pub sort_order: i32,
    pub due_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct TaskUpdate {
    pub content: String,
    pub description: Option<String>,
    pub project_id: Option<String>,
    pub is_completed: bool,
    pub sort_order: i32,
    pub due_at: Option<DateTime<Utc>>,
}

pub async fn list(
    db: &Db,
    owner: &str,
    project_id: Option<&str>,
    completed: Option<bool>,
) -> Result<Vec<Task>, DaybookError> {
    let schema = schema();
    let sql = format!(
        "SELECT {COLUMNS} FROM {schema}.tasks WHERE owner = $1 AND (($2::text IS NULL AND project_id IS NULL) OR project_id = $2) AND ($3::bool IS NULL OR is_completed = $3) ORDER BY sort_order, created_at"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(owner)
        .bind(project_id)
        .bind(completed)
        .fetch_all(pool(db)?)
        .await?;
    rows.iter().map(from_row).collect()
}

pub async fn create(db: &Db, owner: &str, new: &NewTask) -> Result<Task, DaybookError> {
    let schema = schema();
    let sql = format!(
        "INSERT INTO {schema}.tasks (id, owner, project_id, content, description, sort_order, due_at) VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {COLUMNS}"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(Uuid::new_v4().to_string())
        .bind(owner)
        .bind(&new.project_id)
        .bind(&new.content)
        .bind(&new.description)
        .bind(new.sort_order)
        .bind(new.due_at)
        .fetch_one(pool(db)?)
        .await?;
    from_row(&row)
}

pub async fn update(
    db: &Db,
    owner: &str,
    id: &str,
    changes: &TaskUpdate,
) -> Result<Option<Task>, DaybookError> {
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.tasks SET project_id = $3, content = $4, description = $5, is_completed = $6, sort_order = $7, due_at = $8, updated_at = NOW() WHERE id = $1 AND owner = $2 RETURNING {COLUMNS}"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(owner)
        .bind(&changes.project_id)
        .bind(&changes.content)
        .bind(&changes.description)
        .bind(changes.is_completed)
        .bind(changes.sort_order)
        .bind(changes.due_at)
        .fetch_optional(pool(db)?)
        .await?;
    row.as_ref().map(from_row).transpose()
}

pub async fn delete(db: &Db, owner: &str, id: &str) -> Result<bool, DaybookError> {
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.tasks WHERE id = $1 AND owner = $2");
    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(owner)
        .execute(pool(db)?)
        .await?;
    Ok(result.rows_affected() > 0)
}

const COLUMNS: &str = "id, project_id, content, description, is_completed, sort_order, due_at, created_at, updated_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Task, DaybookError> {
    Ok(Task {
        id: row.try_get("id")?,
        project_id: row.try_get("project_id")?,
        content: row.try_get("content")?,
        description: row.try_get("description")?,
        is_completed: row.try_get("is_completed")?,
        sort_order: row.try_get("sort_order")?,
        due_at: row.try_get("due_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
