//! Projects: the flat container every issue lives in. Flat (unlike
//! conveyor's tree) keeps the permission check a single lookup, not a walk.

use crate::domain::db::{WorkbenchError, pool, schema};
use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_db::prelude::Db;
use quench_serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    /// Short code issue keys are built from (`WB-1`, ...); unique per schema.
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: Time,
    pub updated_at: Time,
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

    let row = pool
        .query_one(&sql, &[&id, &new.key, &new.name, &new.description])
        .await?;

    from_row(&row)
}

pub async fn read(db: &Db, id: &str) -> Result<Option<Project>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.projects WHERE id = $1");

    let row = pool.query_opt(&sql, &[&id]).await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn read_by_key(db: &Db, key: &str) -> Result<Option<Project>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.projects WHERE key = $1");

    let row = pool.query_opt(&sql, &[&key]).await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn list(db: &Db) -> Result<Vec<Project>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.projects ORDER BY key");

    let rows = pool.query(&sql, &[]).await?;

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

    let row = pool
        .query_opt(&sql, &[&id, &changes.name, &changes.description])
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.projects WHERE id = $1");

    let result = pool.execute(&sql, &[&id]).await?;

    Ok(result > 0)
}

const COLUMNS: &str = "id, key, name, description, created_at, updated_at";

fn from_row(row: &quench_pg::Row) -> Result<Project, WorkbenchError> {
    Ok(Project {
        id: row.get("id")?,
        key: row.get("key")?,
        name: row.get("name")?,
        description: row.get("description")?,
        created_at: row.get::<Time, _>("created_at")?,
        updated_at: row.get::<Time, _>("updated_at")?,
    })
}
