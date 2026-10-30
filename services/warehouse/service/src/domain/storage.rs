//! Dynamic, admin-provisioned, quota'd storages - the DB counterpart to `routers::files`'s static ones.

use crate::domain::db::{StorageError, pool, schema};
use quench_core::time::Time;
use quench_db::prelude::Db;
use quench_serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DynamicStorage {
    pub name: String,
    pub owner: String,
    /// `None` falls back to the deployment-wide default.
    pub max_file_bytes: Option<i64>,
    pub quota_bytes: i64,
    pub used_bytes: i64,
    pub sync_enabled: bool,
    pub created_at: Time,
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

    let row = pool
        .query_one(
            &sql,
            &[
                &new.name as &dyn quench_pg::ToSql,
                &new.owner,
                &new.max_file_bytes,
                &new.quota_bytes,
                &new.sync_enabled,
            ],
        )
        .await?;

    from_row(&row)
}

pub async fn read(db: &Db, name: &str) -> Result<Option<DynamicStorage>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.storages WHERE name = $1");

    let row = pool
        .query_opt(&sql, &[&name as &dyn quench_pg::ToSql])
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// Every dynamic storage; caller-visibility filtering happens in the route handler.
pub async fn list(db: &Db) -> Result<Vec<DynamicStorage>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.storages ORDER BY name");

    let rows = pool.query(&sql, &[]).await?;

    rows.iter().map(from_row).collect()
}

#[derive(Clone, Debug, Default)]
pub struct StorageUpdate {
    pub max_file_bytes: Option<Option<i64>>,
    pub quota_bytes: Option<i64>,
    pub sync_enabled: Option<bool>,
}

/// Applies whichever fields `changes` sets. `max_file_bytes`: outer `None` = leave alone, inner = clear.
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

    let row = pool
        .query_opt(
            &sql,
            &[
                &name as &dyn quench_pg::ToSql,
                &max_file_bytes,
                &quota_bytes,
                &sync_enabled,
            ],
        )
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// Deletes the storage row (cascades to its files/sync-log). Caller must release blob ref-counts first.
pub async fn delete(db: &Db, name: &str) -> Result<bool, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.storages WHERE name = $1");

    let result = pool
        .execute(&sql, &[&name as &dyn quench_pg::ToSql])
        .await?;

    Ok(result > 0)
}

const COLUMNS: &str =
    "name, owner, max_file_bytes, quota_bytes, used_bytes, sync_enabled, created_at";

fn from_row(row: &quench_pg::Row) -> Result<DynamicStorage, StorageError> {
    Ok(DynamicStorage {
        name: row.get("name")?,
        owner: row.get("owner")?,
        max_file_bytes: row.get("max_file_bytes")?,
        quota_bytes: row.get("quota_bytes")?,
        used_bytes: row.get("used_bytes")?,
        sync_enabled: row.get("sync_enabled")?,
        created_at: row.get::<Time, _>("created_at")?,
    })
}
