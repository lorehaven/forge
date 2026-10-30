//! One dynamic storage's content: `storage_files`, the `blobs` they dedup through, and the sync log.
//! `put_file`/`delete_file` move blob files inside the same locked DB transaction to avoid a race.

use crate::domain::db::{StorageError, pool, schema};
use quench_core::time::Time;
use quench_db::prelude::Db;
use std::path::Path;

/// What [`put_file`] did, for the handler to turn into a 200 vs. 201.
#[derive(Debug)]
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
    let mut conn = pool.get().await?;
    let mut tx = conn.begin().await?;

    lock_for_put(&mut tx, storage_name, sha256).await?;

    let storage_sql = format!(
        "SELECT quota_bytes, used_bytes, sync_enabled FROM {schema}.storages WHERE name = $1"
    );
    let storage_row = tx
        .query_opt(&storage_sql, &[&storage_name as &dyn quench_pg::ToSql])
        .await?
        .ok_or_else(|| StorageError::NoSuchStorage(storage_name.to_string()))?;
    let quota_bytes: i64 = storage_row.get("quota_bytes")?;
    let used_bytes: i64 = storage_row.get("used_bytes")?;
    let sync_enabled: bool = storage_row.get("sync_enabled")?;

    let existing_sql =
        format!("SELECT sha256, size FROM {schema}.storage_files WHERE storage = $1 AND path = $2");
    let existing = tx
        .query_opt(
            &existing_sql,
            &[&storage_name as &dyn quench_pg::ToSql, &path],
        )
        .await?;

    let (old_sha256, old_size): (Option<String>, i64) = if let Some(row) = &existing {
        (Some(row.get("sha256")?), row.get("size")?)
    } else {
        (None, 0)
    };

    let delta = size - old_size;
    if used_bytes + delta > quota_bytes {
        tx.rollback().await?;
        return Err(StorageError::QuotaExceeded);
    }

    // Re-uploading identical bytes to the same path must not bump ref_count again (it'd never hit zero).
    let adding_reference = old_sha256.as_deref() != Some(sha256);

    if adding_reference {
        add_blob_reference(&mut tx, &schema, sha256, size, staging, blob_path).await?;

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
    tx.execute(
        &upsert_sql,
        &[
            &storage_name as &dyn quench_pg::ToSql,
            &path,
            &sha256,
            &size,
        ],
    )
    .await?;

    let quota_sql =
        format!("UPDATE {schema}.storages SET used_bytes = used_bytes + $2 WHERE name = $1");
    tx.execute(
        &quota_sql,
        &[&storage_name as &dyn quench_pg::ToSql, &delta],
    )
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
    drop(conn);

    Ok(PutOutcome {
        existed: existing.is_some(),
    })
}

/// Serializes writers to this storage, and uploads of the same content across storages.
async fn lock_for_put(
    tx: &mut quench_pg::Transaction<'_>,
    storage_name: &str,
    sha256: &str,
) -> Result<(), StorageError> {
    // Serializes writers to this storage so the quota check sees a consistent `used_bytes`.
    tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&storage_name as &dyn quench_pg::ToSql],
    )
    .await?;
    // Serializes concurrent uploads of the same content across storages.
    tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 1))",
        &[&sha256 as &dyn quench_pg::ToSql],
    )
    .await?;
    Ok(())
}

/// Counts one more reference to the blob, moving `staging` into the store when the blob is new.
async fn add_blob_reference(
    tx: &mut quench_pg::Transaction<'_>,
    schema: &str,
    sha256: &str,
    size: i64,
    staging: &Path,
    blob_path: &Path,
) -> Result<(), StorageError> {
    let blob_sql = format!(
        "INSERT INTO {schema}.blobs (sha256, size, ref_count) VALUES ($1, $2, 1) \
         ON CONFLICT (sha256) DO UPDATE SET ref_count = {schema}.blobs.ref_count + 1 \
         RETURNING ref_count"
    );
    let ref_count: i64 = tx
        .query_one(&blob_sql, &[&sha256 as &dyn quench_pg::ToSql, &size])
        .await?
        .get("ref_count")?;

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
    Ok(())
}

/// Removes `path`, releasing its blob reference and refunding quota. Deletes the file only at `ref_count` 0.
pub async fn delete_file(
    db: &Db,
    storage_name: &str,
    path: &str,
    blob_path_for: impl Fn(&str) -> std::path::PathBuf,
) -> Result<bool, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let mut conn = pool.get().await?;
    let mut tx = conn.begin().await?;

    tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&storage_name as &dyn quench_pg::ToSql],
    )
    .await?;

    let existing_sql =
        format!("SELECT sha256, size FROM {schema}.storage_files WHERE storage = $1 AND path = $2");
    let Some(row) = tx
        .query_opt(
            &existing_sql,
            &[&storage_name as &dyn quench_pg::ToSql, &path],
        )
        .await?
    else {
        tx.rollback().await?;
        return Ok(false);
    };
    let sha256: String = row.get("sha256")?;
    let size: i64 = row.get("size")?;

    let delete_sql = format!("DELETE FROM {schema}.storage_files WHERE storage = $1 AND path = $2");
    tx.execute(
        &delete_sql,
        &[&storage_name as &dyn quench_pg::ToSql, &path],
    )
    .await?;

    let blob_path = blob_path_for(&sha256);
    release_blob(&mut tx, &schema, &sha256, Some(&blob_path)).await?;

    let quota_sql =
        format!("UPDATE {schema}.storages SET used_bytes = used_bytes - $2 WHERE name = $1");
    tx.execute(&quota_sql, &[&storage_name as &dyn quench_pg::ToSql, &size])
        .await?;

    let sync_enabled: bool = tx
        .query_one(
            &format!("SELECT sync_enabled FROM {schema}.storages WHERE name = $1"),
            &[&storage_name as &dyn quench_pg::ToSql],
        )
        .await?
        .get("sync_enabled")?;

    if sync_enabled {
        append_sync_log(&mut tx, &schema, storage_name, path, "delete", None, None).await?;
    }

    tx.commit().await?;
    drop(conn);

    Ok(true)
}

/// Decrements a blob's `ref_count`, deleting its row (and file, if `blob_path` given) at zero.
/// `None` from `put_file`'s overwrite case just leaves a harmless leak, cleaned up lazily later.
async fn release_blob(
    tx: &mut quench_pg::Transaction<'_>,
    schema: &str,
    sha256: &str,
    blob_path: Option<&Path>,
) -> Result<(), StorageError> {
    let sql = format!(
        "UPDATE {schema}.blobs SET ref_count = ref_count - 1 WHERE sha256 = $1 RETURNING ref_count"
    );
    let ref_count: i64 = tx
        .query_one(&sql, &[&sha256 as &dyn quench_pg::ToSql])
        .await?
        .get("ref_count")?;

    if ref_count <= 0 {
        let delete_sql = format!("DELETE FROM {schema}.blobs WHERE sha256 = $1");
        tx.execute(&delete_sql, &[&sha256 as &dyn quench_pg::ToSql])
            .await?;

        if let Some(blob_path) = blob_path {
            let _ = tokio::fs::remove_file(blob_path).await;
        }
    }

    Ok(())
}

#[derive(Clone, Debug, quench_serde::Serialize)]
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

    let rows = pool
        .query(&sql, &[&storage_name as &dyn quench_pg::ToSql, &pattern])
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(StorageFile {
                path: row.get("path")?,
                size: row.get("size")?,
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

    let rows = pool
        .query(
            &sql,
            &[
                &storage_name as &dyn quench_pg::ToSql,
                &prefix,
                &lower_bound,
                &upper_bound,
                &after,
                &limit,
            ],
        )
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(StorageFile {
                path: row.get("path")?,
                size: row.get("size")?,
            })
        })
        .collect()
}

/// Exclusive upper bound of `[prefix, _)` under `C` collation: last byte incremented.
/// `None` when no successor exists or bumping would break UTF-8; the query then omits the upper bound.
#[must_use]
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

    let row = pool
        .query_opt(&sql, &[&storage_name as &dyn quench_pg::ToSql, &path])
        .await?;

    row.map(|row| Ok((row.get("sha256")?, row.get("size")?)))
        .transpose()
}

#[derive(Clone, Debug, quench_serde::Serialize)]
pub struct SyncLogEntry {
    pub id: i64,
    pub path: String,
    pub op: String,
    pub sha256: Option<String>,
    pub size: Option<i64>,
    pub at: Time,
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

    let rows = pool
        .query(&sql, &[&storage_name as &dyn quench_pg::ToSql, &since])
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(SyncLogEntry {
                id: row.get("id")?,
                path: row.get("path")?,
                op: row.get("op")?,
                sha256: row.get("sha256")?,
                size: row.get("size")?,
                at: row.get::<Time, _>("at")?,
            })
        })
        .collect()
}

async fn append_sync_log(
    tx: &mut quench_pg::Transaction<'_>,
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
    tx.execute(
        &sql,
        &[
            &storage_name as &dyn quench_pg::ToSql,
            &path,
            &op,
            &sha256,
            &size,
        ],
    )
    .await?;

    Ok(())
}

/// Wraps a filesystem error as the SQL variant for one `?`-friendly error type.
const fn sqlx_io_error(error: std::io::Error) -> StorageError {
    StorageError::Sql(quench_pg::Error::Io(error))
}
