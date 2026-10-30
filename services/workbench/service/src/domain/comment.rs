//! Comments on an issue.

use crate::domain::db::{WorkbenchError, pool, schema};
use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_db::prelude::Db;
use quench_serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub issue_id: String,
    pub author: String,
    pub body: String,
    pub created_at: Time,
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

    let row = pool
        .query_one(&sql, &[&id, &new.issue_id, &new.author, &new.body])
        .await?;

    from_row(&row)
}

/// Used before delete to resolve the project id for auth - the row has none.
pub async fn read(db: &Db, id: &str) -> Result<Option<Comment>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.comments WHERE id = $1");

    let row = pool.query_opt(&sql, &[&id]).await?;

    row.as_ref().map(from_row).transpose()
}

/// An issue's comments, oldest first - the order a comment thread reads in.
pub async fn list_by_issue(db: &Db, issue_id: &str) -> Result<Vec<Comment>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql =
        format!("SELECT {COLUMNS} FROM {schema}.comments WHERE issue_id = $1 ORDER BY created_at");

    let rows = pool.query(&sql, &[&issue_id]).await?;

    rows.iter().map(from_row).collect()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.comments WHERE id = $1");

    let result = pool.execute(&sql, &[&id]).await?;

    Ok(result > 0)
}

const COLUMNS: &str = "id, issue_id, author, body, created_at";

fn from_row(row: &quench_pg::Row) -> Result<Comment, WorkbenchError> {
    Ok(Comment {
        id: row.get("id")?,
        issue_id: row.get("issue_id")?,
        author: row.get("author")?,
        body: row.get("body")?,
        created_at: row.get::<Time, _>("created_at")?,
    })
}
