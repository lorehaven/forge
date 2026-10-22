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
    pub remind_at: Option<DateTime<Utc>>,
    /// The task's labels, ordered by label name.
    pub label_ids: Vec<String>,
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
    pub remind_at: Option<DateTime<Utc>>,
    pub label_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TaskUpdate {
    pub content: String,
    pub description: Option<String>,
    pub project_id: Option<String>,
    pub is_completed: bool,
    pub sort_order: i32,
    pub due_at: Option<DateTime<Utc>>,
    pub remind_at: Option<DateTime<Utc>>,
    pub label_ids: Vec<String>,
}

/// Lists the caller's tasks.
///
/// With no `label_id` that is one list: the inbox when `project_id` is `None`, else that project. With a
/// `label_id` it is every task carrying that label, across the inbox and all projects - narrowed to
/// `project_id` only when one is given. `all` lifts the one-list restriction without needing a label.
pub async fn list(
    db: &Db,
    owner: &str,
    project_id: Option<&str>,
    label_id: Option<&str>,
    all: bool,
    completed: Option<bool>,
) -> Result<Vec<Task>, DaybookError> {
    let schema = schema();
    let sql = format!(
        "SELECT {} FROM {schema}.tasks WHERE owner = $1 \
         AND (($5::bool AND $2::text IS NULL) OR ($4::text IS NOT NULL AND $2::text IS NULL) OR ($2::text IS NULL AND project_id IS NULL) OR project_id = $2) \
         AND ($3::bool IS NULL OR is_completed = $3) \
         AND ($4::text IS NULL OR EXISTS (SELECT 1 FROM {schema}.task_labels tl WHERE tl.task_id = tasks.id AND tl.label_id = $4)) \
         ORDER BY sort_order, created_at",
        columns(&schema)
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(owner)
        .bind(project_id)
        .bind(completed)
        .bind(label_id)
        .bind(all)
        .fetch_all(pool(db)?)
        .await?;
    rows.iter().map(from_row).collect()
}

pub async fn create(db: &Db, owner: &str, new: &NewTask) -> Result<Task, DaybookError> {
    let schema = schema();
    let id = Uuid::new_v4().to_string();
    let mut tx = pool(db)?.begin().await?;
    let sql = format!(
        "INSERT INTO {schema}.tasks (id, owner, project_id, content, description, sort_order, due_at, remind_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&id)
        .bind(owner)
        .bind(&new.project_id)
        .bind(&new.content)
        .bind(&new.description)
        .bind(new.sort_order)
        .bind(new.due_at)
        .bind(new.remind_at)
        .execute(&mut *tx)
        .await?;
    set_labels(&mut tx, &schema, owner, &id, &new.label_ids).await?;
    let task = fetch(&mut tx, &schema, owner, &id).await?;
    tx.commit().await?;
    task.ok_or(DaybookError::Sql(sqlx::Error::RowNotFound))
}

pub async fn update(
    db: &Db,
    owner: &str,
    id: &str,
    changes: &TaskUpdate,
) -> Result<Option<Task>, DaybookError> {
    let schema = schema();
    let mut tx = pool(db)?.begin().await?;
    let sql = format!(
        "UPDATE {schema}.tasks SET project_id = $3, content = $4, description = $5, is_completed = $6, sort_order = $7, due_at = $8, remind_at = $9, updated_at = NOW() WHERE id = $1 AND owner = $2"
    );
    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(owner)
        .bind(&changes.project_id)
        .bind(&changes.content)
        .bind(&changes.description)
        .bind(changes.is_completed)
        .bind(changes.sort_order)
        .bind(changes.due_at)
        .bind(changes.remind_at)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    set_labels(&mut tx, &schema, owner, id, &changes.label_ids).await?;
    let task = fetch(&mut tx, &schema, owner, id).await?;
    tx.commit().await?;
    Ok(task)
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

/// Replaces the task's label set. A label that is not the caller's fails the composite foreign key
/// (`23503`), which the router reports as a bad request.
async fn set_labels(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    schema: &str,
    owner: &str,
    task_id: &str,
    label_ids: &[String],
) -> Result<(), DaybookError> {
    let sql = format!("DELETE FROM {schema}.task_labels WHERE task_id = $1 AND owner = $2");
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(task_id)
        .bind(owner)
        .execute(&mut **tx)
        .await?;
    if label_ids.is_empty() {
        return Ok(());
    }
    let mut unique: Vec<&str> = Vec::with_capacity(label_ids.len());
    for id in label_ids {
        if !unique.contains(&id.as_str()) {
            unique.push(id);
        }
    }
    let sql = format!(
        "INSERT INTO {schema}.task_labels (task_id, label_id, owner) SELECT $1, label_id, $2 FROM unnest($3::text[]) AS label_id"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(task_id)
        .bind(owner)
        .bind(&unique)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn fetch(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    schema: &str,
    owner: &str,
    id: &str,
) -> Result<Option<Task>, DaybookError> {
    let sql = format!(
        "SELECT {} FROM {schema}.tasks WHERE id = $1 AND owner = $2",
        columns(schema)
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(owner)
        .fetch_optional(&mut **tx)
        .await?;
    row.as_ref().map(from_row).transpose()
}

/// The column list, with the task's label ids folded into one array column.
fn columns(schema: &str) -> String {
    format!(
        "id, project_id, content, description, is_completed, sort_order, due_at, remind_at, \
         COALESCE((SELECT array_agg(l.id ORDER BY lower(l.name), l.id) FROM {schema}.task_labels tl \
         JOIN {schema}.labels l ON l.id = tl.label_id WHERE tl.task_id = tasks.id), ARRAY[]::text[]) AS label_ids, \
         created_at, updated_at"
    )
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Task, DaybookError> {
    Ok(Task {
        id: row.try_get("id")?,
        project_id: row.try_get("project_id")?,
        content: row.try_get("content")?,
        description: row.try_get("description")?,
        is_completed: row.try_get("is_completed")?,
        sort_order: row.try_get("sort_order")?,
        due_at: row.try_get("due_at")?,
        remind_at: row.try_get("remind_at")?,
        label_ids: row.try_get("label_ids")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
