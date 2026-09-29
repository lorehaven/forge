//! Who follows which repository or project. Raw SQL, like the rest of the scheduler's tables.

use crate::scheduler::projects;
use crate::scheduler::queue::{QueueError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

/// What a subscription follows. A project covers every repository nested beneath it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope<'a> {
    Repo(&'a str),
    Project(&'a str),
}

impl<'a> Scope<'a> {
    const fn column(self) -> &'static str {
        match self {
            Self::Repo(_) => "repo_id",
            Self::Project(_) => "project_id",
        }
    }

    const fn id(self) -> &'a str {
        match self {
            Self::Repo(id) | Self::Project(id) => id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Subscription {
    pub id: String,
    pub repo_id: Option<String>,
    pub project_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Idempotent: `true` when this made the subscription, `false` when it already existed.
pub async fn subscribe(db: &Db, username: &str, scope: Scope<'_>) -> Result<bool, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let column = scope.column();
    let sql = format!(
        "INSERT INTO {schema}.subscriptions (id, username, {column}) \
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
    );

    let done = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(Uuid::new_v4().to_string())
        .bind(username)
        .bind(scope.id())
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// `true` when there was a subscription to remove.
pub async fn unsubscribe(db: &Db, username: &str, scope: Scope<'_>) -> Result<bool, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let column = scope.column();
    let sql = format!("DELETE FROM {schema}.subscriptions WHERE username = $1 AND {column} = $2");

    let done = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(username)
        .bind(scope.id())
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn list_for_user(db: &Db, username: &str) -> Result<Vec<Subscription>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT id, repo_id, project_id, created_at FROM {schema}.subscriptions \
         WHERE username = $1 ORDER BY created_at, id"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(username)
        .fetch_all(pool)
        .await?;

    rows.iter()
        .map(|row| {
            Ok(Subscription {
                id: row.try_get("id")?,
                repo_id: row.try_get("repo_id")?,
                project_id: row.try_get("project_id")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect()
}

/// Everyone following this repository directly or through a project above it, once each.
pub async fn subscribers(
    db: &Db,
    repo_id: &str,
    project_id: &str,
) -> Result<Vec<String>, QueueError> {
    let chain = projects::ancestor_chain(db, project_id).await?;
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT DISTINCT username FROM {schema}.subscriptions \
         WHERE repo_id = $1 OR project_id = ANY($2) ORDER BY username"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(repo_id)
        .bind(&chain)
        .fetch_all(pool)
        .await?;

    rows.iter()
        .map(|row| {
            row.try_get::<String, _>("username")
                .map_err(QueueError::from)
        })
        .collect()
}
