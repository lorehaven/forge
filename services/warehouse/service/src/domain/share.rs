//! Share links: a file, or a directory with everything below it, opened by a link and no sign-in.
//!
//! The token in the link is the whole credential. It is 256 random bits, never derived from the path or the person, and
//! it is the only thing a lookup goes by. A share is removed by its creator (or by whoever may write everywhere); it
//! goes with its storage, and with its creator's account.

use crate::domain::db::{StorageError, pool, schema};
use quench_core::base64::URL_SAFE_NO_PAD;
use quench_core::time::Time;
use quench_db::prelude::Db;
use quench_serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Share {
    pub id: String,
    pub token: String,
    pub storage: String,
    /// Relative to the storage; empty is the storage itself.
    pub path: String,
    pub is_dir: bool,
    pub created_by: String,
    pub created_at: Time,
}

const COLUMNS: &str = "id, token, storage, path, is_dir, created_by, created_at";

/// A fresh token: 32 random bytes, URL-safe, 43 characters.
#[must_use]
pub fn new_token() -> String {
    URL_SAFE_NO_PAD.encode(quench_core::random::bytes::<32>())
}

fn from_row(row: &quench_pg::Row) -> Result<Share, StorageError> {
    Ok(Share {
        id: row.get("id")?,
        token: row.get("token")?,
        storage: row.get("storage")?,
        path: row.get("path")?,
        is_dir: row.get("is_dir")?,
        created_by: row.get("created_by")?,
        created_at: row.get::<Time, _>("created_at")?,
    })
}

/// The share of `path` this person already made, or a new one: asking twice gives the same link.
pub async fn create(
    db: &Db,
    storage: &str,
    path: &str,
    is_dir: bool,
    created_by: &str,
) -> Result<Share, StorageError> {
    let pool = pool(db)?;
    let schema = schema();

    let existing = format!(
        "SELECT {COLUMNS} FROM {schema}.shares \
         WHERE storage = $1 AND path = $2 AND is_dir = $3 AND created_by = $4"
    );
    if let Some(row) = pool
        .query_opt(
            &existing,
            &[
                &storage as &dyn quench_pg::ToSql,
                &path,
                &is_dir,
                &created_by,
            ],
        )
        .await?
    {
        return from_row(&row);
    }

    let sql = format!(
        "INSERT INTO {schema}.shares (id, token, storage, path, is_dir, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLUMNS}"
    );
    let id = quench_core::uuid::Uuid::new_v4().to_string();
    let token = new_token();
    let row = pool
        .query_one(
            &sql,
            &[
                &id as &dyn quench_pg::ToSql,
                &token,
                &storage,
                &path,
                &is_dir,
                &created_by,
            ],
        )
        .await?;
    from_row(&row)
}

/// The share a link names.
pub async fn by_token(db: &Db, token: &str) -> Result<Option<Share>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.shares WHERE token = $1");
    let row = pool
        .query_opt(&sql, &[&token as &dyn quench_pg::ToSql])
        .await?;
    row.as_ref().map(from_row).transpose()
}

pub async fn read(db: &Db, id: &str) -> Result<Option<Share>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.shares WHERE id = $1");
    let row = pool
        .query_opt(&sql, &[&id as &dyn quench_pg::ToSql])
        .await?;
    row.as_ref().map(from_row).transpose()
}

/// Newest first: everyone's with `created_by` as `None`, otherwise one person's.
pub async fn list(db: &Db, created_by: Option<&str>) -> Result<Vec<Share>, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT {COLUMNS} FROM {schema}.shares \
         WHERE ($1::text IS NULL OR created_by = $1) ORDER BY created_at DESC, id"
    );
    let rows = pool
        .query(&sql, &[&created_by as &dyn quench_pg::ToSql])
        .await?;
    rows.iter().map(from_row).collect()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.shares WHERE id = $1");
    Ok(pool.execute(&sql, &[&id as &dyn quench_pg::ToSql]).await? > 0)
}

/// Every share of a storage that is going away.
pub async fn delete_for_storage(db: &Db, storage: &str) -> Result<u64, StorageError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.shares WHERE storage = $1");
    Ok(pool
        .execute(&sql, &[&storage as &dyn quench_pg::ToSql])
        .await?)
}
