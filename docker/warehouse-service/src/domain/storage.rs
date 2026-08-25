//! Dynamic storages: admin-provisioned, owned, quota'd storages backed by the
//! database - the counterpart to the static, env-configured storages in
//! `routers::files` (`FILE_STORAGES`), which this table never touches.

use crate::domain::db::{StorageError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DynamicStorage {
    pub name: String,
    pub owner: String,
    /// `None` falls back to the deployment-wide default
    /// (`routers::files::max_file_bytes`).
    pub max_file_bytes: Option<i64>,
    pub quota_bytes: i64,
    pub used_bytes: i64,
    pub sync_enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct NewStorage {
    pub name: String,
    pub owner: String,
    pub max_file_bytes: Option<i64>,
    pub quota_bytes: i64,
    pub sync_enabled: bool,
}

pub async fn create(db: &Db, new: &NewStorage) -> Result<DynamicStorage, StorageError> {
    let pool = pool(db)?;
    let schema = schema();

    let sql = format!(
        "INSERT INTO {schema}.storages (name, owner, max_file_bytes, quota_bytes, sync_enabled) \
         VALUES ($1, $2, $3, $4, $5) RETURNING {COLUMNS}"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&new.name)
        .bind(&new.owner)
        .bind(new.max_file_bytes)
        .bind(new.quota_bytes)
        .bind(new.sync_enabled)
        .fetch_one(pool)
        .await?;

    from_row(&row)
}

pub async fn read(db: &Db, name: &str) -> Result<Option<DynamicStorage>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.storages WHERE name = $1");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(name)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// Every dynamic storage this deployment holds - for `GET /api/v1/files`,
/// filtered down to what the caller may see by the route handler, not here.
pub async fn list(db: &Db) -> Result<Vec<DynamicStorage>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.storages ORDER BY name");

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .fetch_all(pool)
        .await?;

    rows.iter().map(from_row).collect()
}

#[derive(Clone, Debug, Default)]
pub struct StorageUpdate {
    pub max_file_bytes: Option<Option<i64>>,
    pub quota_bytes: Option<i64>,
    pub sync_enabled: Option<bool>,
}

/// Applies whichever fields `changes` sets, leaving the rest as they are.
///
/// `max_file_bytes` is `Option<Option<i64>>` because the column itself is
/// nullable: the outer `None` means "leave it alone", the inner `None` means
/// "clear it back to the deployment default".
pub async fn update(
    db: &Db,
    name: &str,
    changes: &StorageUpdate,
) -> Result<Option<DynamicStorage>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();

    let Some(existing) = read(db, name).await? else {
        return Ok(None);
    };

    let max_file_bytes = changes.max_file_bytes.unwrap_or(existing.max_file_bytes);
    let quota_bytes = changes.quota_bytes.unwrap_or(existing.quota_bytes);
    let sync_enabled = changes.sync_enabled.unwrap_or(existing.sync_enabled);

    let sql = format!(
        "UPDATE {schema}.storages SET max_file_bytes = $2, quota_bytes = $3, sync_enabled = $4 \
         WHERE name = $1 RETURNING {COLUMNS}"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(name)
        .bind(max_file_bytes)
        .bind(quota_bytes)
        .bind(sync_enabled)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// Deletes the storage row - cascading, via the foreign keys in the
/// migration, to its `storage_files` and `storage_sync_log` rows. Blob
/// ref-counts for whatever it referenced are the caller's job to release
/// first (see `domain::storage_file::delete_all_for_storage`), since a blob
/// is shared with other storages and this table knows nothing about it.
pub async fn delete(db: &Db, name: &str) -> Result<bool, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.storages WHERE name = $1");

    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(name)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

const COLUMNS: &str =
    "name, owner, max_file_bytes, quota_bytes, used_bytes, sync_enabled, created_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<DynamicStorage, StorageError> {
    Ok(DynamicStorage {
        name: row.try_get("name")?,
        owner: row.try_get("owner")?,
        max_file_bytes: row.try_get("max_file_bytes")?,
        quota_bytes: row.try_get("quota_bytes")?,
        used_bytes: row.try_get("used_bytes")?,
        sync_enabled: row.try_get("sync_enabled")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
    })
}
