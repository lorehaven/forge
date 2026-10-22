use crate::domain::db::{DaybookError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct Label {
    pub id: String,
    pub name: String,
    /// `#rrggbb`.
    pub color: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct NewLabel {
    pub name: String,
    pub color: String,
}

pub async fn list(db: &Db, owner: &str) -> Result<Vec<Label>, DaybookError> {
    let schema = schema();
    let sql =
        format!("SELECT {COLUMNS} FROM {schema}.labels WHERE owner = $1 ORDER BY lower(name), id");
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(owner)
        .fetch_all(pool(db)?)
        .await?;
    rows.iter().map(from_row).collect()
}

pub async fn create(db: &Db, owner: &str, new: &NewLabel) -> Result<Label, DaybookError> {
    let schema = schema();
    let sql = format!(
        "INSERT INTO {schema}.labels (id, owner, name, color) VALUES ($1, $2, $3, $4) RETURNING {COLUMNS}"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(Uuid::new_v4().to_string())
        .bind(owner)
        .bind(&new.name)
        .bind(&new.color)
        .fetch_one(pool(db)?)
        .await?;
    from_row(&row)
}

pub async fn update(
    db: &Db,
    owner: &str,
    id: &str,
    new: &NewLabel,
) -> Result<Option<Label>, DaybookError> {
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.labels SET name = $3, color = $4 WHERE id = $1 AND owner = $2 RETURNING {COLUMNS}"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(owner)
        .bind(&new.name)
        .bind(&new.color)
        .fetch_optional(pool(db)?)
        .await?;
    row.as_ref().map(from_row).transpose()
}

/// Deleting a label detaches it from every task (the join rows cascade); the tasks stay.
pub async fn delete(db: &Db, owner: &str, id: &str) -> Result<bool, DaybookError> {
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.labels WHERE id = $1 AND owner = $2");
    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(owner)
        .execute(pool(db)?)
        .await?;
    Ok(result.rows_affected() > 0)
}

const COLUMNS: &str = "id, name, color, created_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Label, DaybookError> {
    Ok(Label {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        color: row.try_get("color")?,
        created_at: row.try_get("created_at")?,
    })
}
