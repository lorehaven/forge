use crate::domain::db::{DaybookError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct NewProject {
    pub name: String,
    pub sort_order: i32,
}

pub async fn list(db: &Db, owner: &str) -> Result<Vec<Project>, DaybookError> {
    let schema = schema();
    let sql = format!(
        "SELECT {COLUMNS} FROM {schema}.projects WHERE owner = $1 ORDER BY sort_order, name"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(owner)
        .fetch_all(pool(db)?)
        .await?;
    rows.iter().map(from_row).collect()
}

pub async fn create(db: &Db, owner: &str, new: &NewProject) -> Result<Project, DaybookError> {
    let schema = schema();
    let sql = format!(
        "INSERT INTO {schema}.projects (id, owner, name, sort_order) VALUES ($1, $2, $3, $4) RETURNING {COLUMNS}"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(Uuid::new_v4().to_string())
        .bind(owner)
        .bind(&new.name)
        .bind(new.sort_order)
        .fetch_one(pool(db)?)
        .await?;
    from_row(&row)
}

pub async fn update(
    db: &Db,
    owner: &str,
    id: &str,
    new: &NewProject,
) -> Result<Option<Project>, DaybookError> {
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.projects SET name = $3, sort_order = $4 WHERE id = $1 AND owner = $2 RETURNING {COLUMNS}"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(owner)
        .bind(&new.name)
        .bind(new.sort_order)
        .fetch_optional(pool(db)?)
        .await?;
    row.as_ref().map(from_row).transpose()
}

pub async fn delete(db: &Db, owner: &str, id: &str) -> Result<bool, DaybookError> {
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.projects WHERE id = $1 AND owner = $2");
    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(owner)
        .execute(pool(db)?)
        .await?;
    Ok(result.rows_affected() > 0)
}

const COLUMNS: &str = "id, name, sort_order, created_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Project, DaybookError> {
    Ok(Project {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        sort_order: row.try_get("sort_order")?,
        created_at: row.try_get("created_at")?,
    })
}
