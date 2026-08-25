//! One dynamic storage's content: the `storage_files` names, the `blobs`
//! they dedup through, and the `storage_sync_log` change feed a
//! `sync_enabled` storage appends to.
//!
//! Unlike the rest of this domain layer, [`put_file`] and [`delete_file`] also
//! touch the filesystem, not just Postgres: whether a blob is new - and
//! therefore whether its bytes need writing at all - is a fact this same
//! transaction decides, under the storage's and the content's advisory locks.
//! Deciding it here and moving the file from the caller's staging path in a
//! second step would let a concurrent upload of the same content observe a
//! blob row with no file behind it yet. The move itself is a same-filesystem
//! `rename` - metadata-only, not a copy - so holding the transaction open for
//! it costs nothing worth avoiding.

use crate::domain::db::{StorageError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use sqlx::Row;
use std::path::Path;

/// What [`put_file`] did, for the handler to turn into a 200 vs. 201.
pub struct PutOutcome {
    pub existed: bool,
}

/// Records `path` in `storage_name` as pointing at `sha256`, dedup-ing
/// against any existing blob with the same digest and enforcing the
/// storage's quota against the *logical* size - a dedup hit still costs the
/// uploader their quota, so there is no incentive to game it by re-uploading
/// content someone else already stored.
///
/// `staging` is moved into the blob store at `blob_path` when `sha256` is new
/// to this deployment; when it already exists, `staging` is left for the
/// caller to remove, since the bytes it holds are already on disk under a
/// different upload's name.
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

    // Serializes writers to this storage (the quota check and update below
    // need to see a consistent `used_bytes`).
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(storage_name)
        .execute(&mut *tx)
        .await?;
    // Serializes concurrent uploads of the *same content*, wherever they land -
    // two storages backing up the same photo at once must not both decide the
    // blob is new.
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

    // Whether this path is picking up a reference it didn't already hold.
    // Re-uploading the same bytes to a path that already pointed at this
    // exact blob adds nothing - `storage_files` below is an upsert onto the
    // same (storage, path) key, not a second row - so `blobs.ref_count` must
    // not move for it either, or it would never reach zero on delete and the
    // blob would leak forever.
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

        // `ref_count == 1` means this row was just inserted rather than
        // bumped: a blob's row is deleted the moment its count reaches zero
        // (see `release_blob`), so an existing row is never seen at zero
        // here. Anything else is a dedup hit - the bytes are already on disk
        // under a different reference, so `staging` is discarded rather than
        // moved.
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
            // `adding_reference` already established this differs from `sha256`.
            release_blob(&mut tx, &schema, old_sha256, None).await?;
        }
    } else {
        // Unchanged re-upload: the blob is already fully accounted for by
        // the existing `storage_files` row, so this path picks up no new
        // reference and the freshly streamed (identical) bytes are redundant.
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

/// Removes `path` from `storage_name`, releasing its blob reference and
/// refunding the quota it held. `blob_root` is only consulted (to delete the
/// underlying file) if the release drops the blob's `ref_count` to zero.
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

/// Decrements a blob's `ref_count`, deleting its row - and, if `blob_path` is
/// given, its on-disk file - once the count reaches zero.
///
/// `blob_path` is `None` from [`put_file`]'s overwrite case: the caller there
/// doesn't yet know the blob store root at the point this runs and passing it
/// through would mean threading a path two call frames deeper for a case that
/// is not on the hot path (overwriting a path with different content). A
/// row left at `ref_count = 0` with a file still on disk is a harmless leak,
/// not a correctness bug - nothing reads a blob without a `storage_files` row
/// pointing at it - so it is cleaned up lazily, the next time anything calls
/// this with a path for the same digest.
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

/// Shallow-or-not doesn't apply here the way it does for a static storage's
/// directory walk: `storage_files` has no notion of directories at all, so
/// every entry whose path starts with `prefix` is a match, regardless of
/// depth.
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

    // `%`/`_` in a caller's own prefix are not treated as wildcards - escaped
    // so `LIKE` only ever matches it as a literal prefix.
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

/// Wraps a filesystem error as the SQL variant so the plumbing-heavy
/// functions above can use one `?`-friendly error type instead of two.
fn sqlx_io_error(error: std::io::Error) -> StorageError {
    StorageError::Sql(sqlx::Error::Io(error))
}
