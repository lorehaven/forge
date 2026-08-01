//! Reading and writing registered repositories.
//!
//! Raw SQL rather than `quench-db`'s `Crud`, for the same reason the run queue
//! is: the rows carry enums and timestamps that would need a `FromRow` shim
//! either way, and having half the schema go through one path and half through
//! another is worse than having all of it go through this one.

use crate::domain::{Provider, Repo};
use crate::scheduler::queue::{QueueError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use sqlx::Row;
use uuid::Uuid;

/// What a caller supplies to register a repository. The id, timestamps and
/// enabled flag are conveyor's to decide.
#[derive(Clone, Debug)]
pub struct NewRepo {
    pub provider: Provider,
    pub owner: String,
    pub name: String,
    pub clone_url: String,
    pub default_branch: String,
    pub registered_by: String,
}

pub async fn create(db: &Db, new: &NewRepo) -> Result<Repo, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let id = Uuid::new_v4().to_string();

    let sql = format!(
        "INSERT INTO {schema}.repos \
         (id, provider, owner, name, clone_url, default_branch, registered_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING {COLUMNS}"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&id)
        .bind(new.provider.as_str())
        .bind(&new.owner)
        .bind(&new.name)
        .bind(&new.clone_url)
        .bind(&new.default_branch)
        .bind(&new.registered_by)
        .fetch_one(pool)
        .await?;

    from_row(&row)
}

pub async fn read(db: &Db, id: &str) -> Result<Option<Repo>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.repos WHERE id = $1");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// Looks a repository up the way a webhook identifies one.
pub async fn find_by_slug(
    db: &Db,
    provider: Provider,
    owner: &str,
    name: &str,
) -> Result<Option<Repo>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT {COLUMNS} FROM {schema}.repos \
         WHERE provider = $1 AND owner = $2 AND name = $3"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(provider.as_str())
        .bind(owner)
        .bind(name)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// Looks a repository up the way the UI does: by `owner/name` alone, with no
/// provider to disambiguate. Unlike `find_by_slug`, this is not exposed to a
/// webhook - a provider identifies itself, a browser URL does not.
pub async fn find_by_owner_name(
    db: &Db,
    owner: &str,
    name: &str,
) -> Result<Option<Repo>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.repos WHERE owner = $1 AND name = $2");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(owner)
        .bind(name)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn list(db: &Db) -> Result<Vec<Repo>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.repos ORDER BY owner, name");

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .fetch_all(pool)
        .await?;

    rows.iter().map(from_row).collect()
}

/// Turns a repository on or off. Disabling keeps its history and stops it
/// accepting triggers, which is what you want for a repository that has gone
/// bad rather than gone away.
pub async fn set_enabled(db: &Db, id: &str, enabled: bool) -> Result<Option<Repo>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.repos SET enabled = $2, updated_at = NOW() \
         WHERE id = $1 RETURNING {COLUMNS}"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(enabled)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.repos WHERE id = $1");

    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

const COLUMNS: &str = "id, provider, owner, name, clone_url, default_branch, \
                       registered_by, enabled, created_at, updated_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Repo, QueueError> {
    let provider: String = row.try_get("provider")?;

    Ok(Repo {
        id: row.try_get("id")?,
        provider: Provider::parse(&provider)
            .ok_or_else(|| QueueError::BadRow(format!("unknown provider '{provider}'")))?,
        owner: row.try_get("owner")?,
        name: row.try_get("name")?,
        clone_url: row.try_get("clone_url")?,
        default_branch: row.try_get("default_branch")?,
        registered_by: row.try_get("registered_by")?,
        enabled: row.try_get("enabled")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}
