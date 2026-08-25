//! Shared plumbing for the dynamic-storage domain: the pool, the schema name,
//! and the one error type raw SQL access here returns.
//!
//! Raw SQL rather than `quench-db`'s generic `Crud`, for the same reason
//! workbench's domain layer is (`docker/workbench-service/src/domain/db.rs`):
//! a dynamic storage's quota check and a blob's ref-count both need a locked
//! read-then-write `Crud` has no way to express, and half the schema going
//! through one path and half through another would be worse than all of it
//! going through this one.

use quench_db::prelude::Db;
use sqlx::{Pool, Postgres};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(
        "dynamic storages need Postgres; this service is running against an \
         in-memory database, where every storage and file would be lost on restart"
    )]
    NotPostgres,

    #[error("storage `{0}` does not exist")]
    NoSuchStorage(String),

    #[error("storage quota exceeded")]
    QuotaExceeded,

    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

impl StorageError {
    /// Whether this is the unique-constraint violation Postgres raises when a
    /// storage name collides with one that already exists.
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::Sql(sqlx::Error::Database(database)) if database.code().as_deref() == Some("23505"))
    }

    /// Whether this is the foreign-key violation Postgres raises when
    /// `owner` names a username that isn't in the realm.
    pub fn is_foreign_key_violation(&self) -> bool {
        matches!(self, Self::Sql(sqlx::Error::Database(database)) if database.code().as_deref() == Some("23503"))
    }
}

/// The pool, or a clear refusal.
///
/// The estate allows an in-memory database for tests. Storages and files on
/// top of one would look like they worked and lose everything on restart, so
/// this says so rather than degrading quietly.
pub fn pool(db: &Db) -> Result<&Pool<Postgres>, StorageError> {
    match db {
        Db::Postgres(postgres) => Ok(postgres.pool()),
        Db::InMemory(_) => Err(StorageError::NotPostgres),
    }
}

pub fn schema() -> String {
    envmnt::get_or("DB_SCHEMA", "warehouse")
}
