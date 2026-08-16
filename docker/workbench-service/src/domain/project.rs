//! Projects: the flat container every issue lives in.
//!
//! Flat, unlike conveyor's project tree - workbench has no need for nesting,
//! and it keeps the resource-scoped permission check (`workbench:project:<id>:<action>`,
//! added in a later stage) a single lookup instead of an ancestor walk.

use crate::domain::db::{WorkbenchError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    /// Short code issue keys are built from (`WB-1`, `WB-2`, ...). Unique
    /// across the estate's one workbench schema.
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct NewProject {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
}

pub async fn create(db: &Db, new: &NewProject) -> Result<Project, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let id = Uuid::new_v4().to_string();

    let sql = format!(
        "INSERT INTO {schema}.projects (id, key, name, description) \
         VALUES ($1, $2, $3, $4) RETURNING {COLUMNS}"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&id)
        .bind(&new.key)
        .bind(&new.name)
        .bind(&new.description)
        .fetch_one(pool)
        .await?;

    from_row(&row)
}

pub async fn read(db: &Db, id: &str) -> Result<Option<Project>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.projects WHERE id = $1");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn read_by_key(db: &Db, key: &str) -> Result<Option<Project>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.projects WHERE key = $1");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(key)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn list(db: &Db) -> Result<Vec<Project>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.projects ORDER BY key");

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .fetch_all(pool)
        .await?;

    rows.iter().map(from_row).collect()
}

#[derive(Clone, Debug)]
pub struct ProjectUpdate {
    pub name: String,
    pub description: Option<String>,
}

pub async fn update(
    db: &Db,
    id: &str,
    changes: &ProjectUpdate,
) -> Result<Option<Project>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.projects SET name = $2, description = $3, updated_at = NOW() \
         WHERE id = $1 RETURNING {COLUMNS}"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(&changes.name)
        .bind(&changes.description)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.projects WHERE id = $1");

    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

const COLUMNS: &str = "id, key, name, description, created_at, updated_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Project, WorkbenchError> {
    Ok(Project {
        id: row.try_get("id")?,
        key: row.try_get("key")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}
