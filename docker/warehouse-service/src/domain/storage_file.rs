//! One dynamic storage's content: `storage_files`, the `blobs` they dedup through, and the sync log.
//! `put_file`/`delete_file` move blob files inside the same locked DB transaction to avoid a race.

use crate::domain::db::{StorageError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use sqlx::Row;
use std::path::Path;

/// What [`put_file`] did, for the handler to turn into a 200 vs. 201.
pub struct PutOutcome {
    pub existed: bool,
}

/// Records `path` as pointing at `sha256`, dedup-ing blobs but still charging quota per-upload.
/// Moves `staging` into the blob store only when `sha256` is new; caller removes it otherwise.
pub async fn put_file(
    db: &Db,
    storage_name: &str,
    path: &str,
    sha256: &str,
    size: i64,
    staging: &Path,
    blob_path: &Path,
) -> Result<PutOutcome, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let mut tx = pool.begin().await?;

    // Serializes writers to this storage so the quota check sees a consistent `used_bytes`.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(storage_name)
        .execute(&mut *tx)
        .await?;
    // Serializes concurrent uploads of the same content across storages.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 1))")
        .bind(sha256)
        .execute(&mut *tx)
        .await?;

    let storage_sql = format!(
        "SELECT quota_bytes, used_bytes, sync_enabled FROM {schema}.storages WHERE name = $1"
    );
    let storage_row = sqlx::query(sqlx::AssertSqlSafe(storage_sql.as_str()))
        .bind(storage_name)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| StorageError::NoSuchStorage(storage_name.to_string()))?;
    let quota_bytes: i64 = storage_row.try_get("quota_bytes")?;
    let used_bytes: i64 = storage_row.try_get("used_bytes")?;
    let sync_enabled: bool = storage_row.try_get("sync_enabled")?;

    let existing_sql =
        format!("SELECT sha256, size FROM {schema}.storage_files WHERE storage = $1 AND path = $2");
    let existing = sqlx::query(sqlx::AssertSqlSafe(existing_sql.as_str()))
        .bind(storage_name)
        .bind(path)
        .fetch_optional(&mut *tx)
        .await?;

    let mut old_sha256: Option<String> = None;
    let mut old_size: i64 = 0;
    if let Some(row) = &existing {
        old_sha256 = Some(row.try_get("sha256")?);
        old_size = row.try_get("size")?;
    }

    let delta = size - old_size;
    if used_bytes + delta > quota_bytes {
        tx.rollback().await?;
        return Err(StorageError::QuotaExceeded);
    }

    // Re-uploading identical bytes to the same path must not bump ref_count again (it'd never hit zero).
    let adding_reference = old_sha256.as_deref() != Some(sha256);

    if adding_reference {
        let blob_sql = format!(
            "INSERT INTO {schema}.blobs (sha256, size, ref_count) VALUES ($1, $2, 1) \
             ON CONFLICT (sha256) DO UPDATE SET ref_count = {schema}.blobs.ref_count + 1 \
             RETURNING ref_count"
        );
        let ref_count: i64 = sqlx::query(sqlx::AssertSqlSafe(blob_sql.as_str()))
            .bind(sha256)
            .bind(size)
            .fetch_one(&mut *tx)
            .await?
            .try_get("ref_count")?;

        // ref_count == 1 means this row was just inserted; otherwise it's a dedup hit, discard staging.
        if ref_count == 1 {
            if let Some(parent) = blob_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(sqlx_io_error)?;
            }
            tokio::fs::rename(staging, blob_path)
                .await
                .map_err(sqlx_io_error)?;
        } else {
            let _ = tokio::fs::remove_file(staging).await;
        }

        if let Some(old_sha256) = &old_sha256 {
            release_blob(&mut tx, &schema, old_sha256, None).await?;
        }
    } else {
        // Unchanged re-upload: already accounted for, streamed bytes are redundant.
        let _ = tokio::fs::remove_file(staging).await;
    }

    let upsert_sql = format!(
        "INSERT INTO {schema}.storage_files (storage, path, sha256, size) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (storage, path) DO UPDATE SET sha256 = $3, size = $4, updated_at = NOW()"
    );
    sqlx::query(sqlx::AssertSqlSafe(upsert_sql.as_str()))
        .bind(storage_name)
        .bind(path)
        .bind(sha256)
        .bind(size)
        .execute(&mut *tx)
        .await?;

    let quota_sql =
        format!("UPDATE {schema}.storages SET used_bytes = used_bytes + $2 WHERE name = $1");
    sqlx::query(sqlx::AssertSqlSafe(quota_sql.as_str()))
        .bind(storage_name)
        .bind(delta)
        .execute(&mut *tx)
        .await?;

    if sync_enabled {
        append_sync_log(
            &mut tx,
            &schema,
            storage_name,
            path,
            "put",
            Some(sha256),
            Some(size),
        )
        .await?;
    }

    tx.commit().await?;

    Ok(PutOutcome {
        existed: existing.is_some(),
    })
}

/// Removes `path`, releasing its blob reference and refunding quota. Deletes the file only at ref_count 0.
pub async fn delete_file(
    db: &Db,
    storage_name: &str,
    path: &str,
    blob_path_for: impl Fn(&str) -> std::path::PathBuf,
) -> Result<bool, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let mut tx = pool.begin().await?;

    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(storage_name)
        .execute(&mut *tx)
        .await?;

    let existing_sql =
        format!("SELECT sha256, size FROM {schema}.storage_files WHERE storage = $1 AND path = $2");
    let Some(row) = sqlx::query(sqlx::AssertSqlSafe(existing_sql.as_str()))
        .bind(storage_name)
        .bind(path)
        .fetch_optional(&mut *tx)
        .await?
    else {
        tx.rollback().await?;
        return Ok(false);
    };
    let sha256: String = row.try_get("sha256")?;
    let size: i64 = row.try_get("size")?;

    let delete_sql = format!("DELETE FROM {schema}.storage_files WHERE storage = $1 AND path = $2");
    sqlx::query(sqlx::AssertSqlSafe(delete_sql.as_str()))
        .bind(storage_name)
        .bind(path)
        .execute(&mut *tx)
        .await?;

    let blob_path = blob_path_for(&sha256);
    release_blob(&mut tx, &schema, &sha256, Some(&blob_path)).await?;

    let quota_sql =
        format!("UPDATE {schema}.storages SET used_bytes = used_bytes - $2 WHERE name = $1");
    sqlx::query(sqlx::AssertSqlSafe(quota_sql.as_str()))
        .bind(storage_name)
        .bind(size)
        .execute(&mut *tx)
        .await?;

    let sync_enabled: bool = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT sync_enabled FROM {schema}.storages WHERE name = $1").as_str(),
    ))
    .bind(storage_name)
    .fetch_one(&mut *tx)
    .await?
    .try_get("sync_enabled")?;

    if sync_enabled {
        append_sync_log(&mut tx, &schema, storage_name, path, "delete", None, None).await?;
    }

    tx.commit().await?;

    Ok(true)
}

/// Decrements a blob's `ref_count`, deleting its row (and file, if `blob_path` given) at zero.
/// `None` from `put_file`'s overwrite case just leaves a harmless leak, cleaned up lazily later.
async fn release_blob(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    schema: &str,
    sha256: &str,
    blob_path: Option<&Path>,
) -> Result<(), StorageError> {
    let sql = format!(
        "UPDATE {schema}.blobs SET ref_count = ref_count - 1 WHERE sha256 = $1 RETURNING ref_count"
    );
    let ref_count: i64 = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(sha256)
        .fetch_one(&mut **tx)
        .await?
        .try_get("ref_count")?;

    if ref_count <= 0 {
        let delete_sql = format!("DELETE FROM {schema}.blobs WHERE sha256 = $1");
        sqlx::query(sqlx::AssertSqlSafe(delete_sql.as_str()))
            .bind(sha256)
            .execute(&mut **tx)
            .await?;

        if let Some(blob_path) = blob_path {
            let _ = tokio::fs::remove_file(blob_path).await;
        }
    }

    Ok(())
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct StorageFile {
    pub path: String,
    pub size: i64,
}

/// No directory notion here - every entry whose path starts with `prefix` matches, any depth.
pub async fn list_files(
    db: &Db,
    storage_name: &str,
    prefix: &str,
) -> Result<Vec<StorageFile>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT path, size FROM {schema}.storage_files \
         WHERE storage = $1 AND path LIKE $2 ORDER BY path"
    );

    // Escape LIKE wildcards so a caller's own `%`/`_` match literally.
    let escaped = prefix
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("{escaped}%");

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(storage_name)
        .bind(pattern)
        .fetch_all(pool)
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(StorageFile {
                path: row.try_get("path")?,
                size: row.try_get("size")?,
            })
        })
        .collect()
}

/// A bounded, resumable page of [`list_files`] for storages with tens of thousands of paths.
/// Ordering/range use the `C` collation to match its index; any other collation forces a full scan.
pub async fn list_files_page(
    db: &Db,
    storage_name: &str,
    prefix: &str,
    after: Option<&str>,
    limit: i64,
    desc: bool,
) -> Result<Vec<StorageFile>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let (cmp, order) = if desc { ("<", "DESC") } else { (">", "ASC") };

    // `starts_with` is the correct predicate; the `$3`/`$4` C-collation range restates it as an
    // index-bound range. Empty prefix drops both bounds via the `IS NULL` guards.
    let sql = format!(
        "SELECT path, size FROM {schema}.storage_files \
         WHERE storage = $1 AND starts_with(path, $2) \
           AND ($3::text IS NULL OR path COLLATE \"C\" >= $3) \
           AND ($4::text IS NULL OR path COLLATE \"C\" < $4) \
           AND ($5::text IS NULL OR path COLLATE \"C\" {cmp} $5) \
         ORDER BY path COLLATE \"C\" {order} LIMIT $6"
    );

    let lower_bound = (!prefix.is_empty()).then(|| prefix.to_string());
    let upper_bound = prefix_upper_bound(prefix);

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(storage_name)
        .bind(prefix)
        .bind(lower_bound)
        .bind(upper_bound)
        .bind(after)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(StorageFile {
                path: row.try_get("path")?,
                size: row.try_get("size")?,
            })
        })
        .collect()
}

/// Exclusive upper bound of `[prefix, _)` under `C` collation: last byte incremented.
/// `None` when no successor exists or bumping would break UTF-8; the query then omits the upper bound.
pub fn prefix_upper_bound(prefix: &str) -> Option<String> {
    let mut bytes = prefix.as_bytes().to_vec();
    while let Some(last) = bytes.last_mut() {
        if *last < 0xFF {
            *last += 1;
            return String::from_utf8(bytes).ok();
        }
        bytes.pop();
    }
    None
}

pub async fn read_file(
    db: &Db,
    storage_name: &str,
    path: &str,
) -> Result<Option<(String, i64)>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql =
        format!("SELECT sha256, size FROM {schema}.storage_files WHERE storage = $1 AND path = $2");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(storage_name)
        .bind(path)
        .fetch_optional(pool)
        .await?;

    row.map(|row| Ok((row.try_get("sha256")?, row.try_get("size")?)))
        .transpose()
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SyncLogEntry {
    pub id: i64,
    pub path: String,
    pub op: String,
    pub sha256: Option<String>,
    pub size: Option<i64>,
    pub at: DateTime<Utc>,
}

pub async fn sync_log_since(
    db: &Db,
    storage_name: &str,
    since: i64,
) -> Result<Vec<SyncLogEntry>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT id, path, op, sha256, size, at FROM {schema}.storage_sync_log \
         WHERE storage = $1 AND id > $2 ORDER BY id"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(storage_name)
        .bind(since)
        .fetch_all(pool)
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(SyncLogEntry {
                id: row.try_get("id")?,
                path: row.try_get("path")?,
                op: row.try_get("op")?,
                sha256: row.try_get("sha256")?,
                size: row.try_get("size")?,
                at: row.try_get::<DateTime<Utc>, _>("at")?,
            })
        })
        .collect()
}

async fn append_sync_log(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    schema: &str,
    storage_name: &str,
    path: &str,
    op: &str,
    sha256: Option<&str>,
    size: Option<i64>,
) -> Result<(), StorageError> {
    let sql = format!(
        "INSERT INTO {schema}.storage_sync_log (storage, path, op, sha256, size) \
         VALUES ($1, $2, $3, $4, $5)"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(storage_name)
        .bind(path)
        .bind(op)
        .bind(sha256)
        .bind(size)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

/// Wraps a filesystem error as the SQL variant for one `?`-friendly error type.
fn sqlx_io_error(error: std::io::Error) -> StorageError {
    StorageError::Sql(sqlx::Error::Io(error))
}
