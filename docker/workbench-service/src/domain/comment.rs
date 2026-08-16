//! Comments on an issue.

use crate::domain::db::{WorkbenchError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub issue_id: String,
    pub author: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct NewComment {
    pub issue_id: String,
    pub author: String,
    pub body: String,
}

pub async fn create(db: &Db, new: &NewComment) -> Result<Comment, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let id = Uuid::new_v4().to_string();

    let sql = format!(
        "INSERT INTO {schema}.comments (id, issue_id, author, body) \
         VALUES ($1, $2, $3, $4) RETURNING {COLUMNS}"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&id)
        .bind(&new.issue_id)
        .bind(&new.author)
        .bind(&new.body)
        .fetch_one(pool)
        .await?;

    from_row(&row)
}

/// Looked up before a delete, to resolve back to the issue (and from there the
/// project) an authorization check needs - a comment's own row carries no
/// project id.
pub async fn read(db: &Db, id: &str) -> Result<Option<Comment>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.comments WHERE id = $1");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// An issue's comments, oldest first - the order a comment thread reads in.
pub async fn list_by_issue(db: &Db, issue_id: &str) -> Result<Vec<Comment>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql =
        format!("SELECT {COLUMNS} FROM {schema}.comments WHERE issue_id = $1 ORDER BY created_at");

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(issue_id)
        .fetch_all(pool)
        .await?;

    rows.iter().map(from_row).collect()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.comments WHERE id = $1");

    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

const COLUMNS: &str = "id, issue_id, author, body, created_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Comment, WorkbenchError> {
    Ok(Comment {
        id: row.try_get("id")?,
        issue_id: row.try_get("issue_id")?,
        author: row.try_get("author")?,
        body: row.try_get("body")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
    })
}
