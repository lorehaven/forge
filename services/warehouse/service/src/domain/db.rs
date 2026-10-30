//! Shared pool/schema/error plumbing for the dynamic-storage domain.
//! Raw SQL, not `Crud`: quota checks and ref-counts need a locked read-then-write `Crud` can't express.

use quench_db::prelude::Db;
use quench_pg::Pool;

#[derive(Debug, quench_core::Error)]
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
    Sql(#[from] quench_pg::Error),
}

impl StorageError {
    /// True for Postgres's unique-constraint violation (duplicate storage name).
    #[must_use]
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::Sql(error) if error.code() == Some("23505"))
    }

    /// True for Postgres's foreign-key violation (`owner` not a real username).
    #[must_use]
    pub fn is_foreign_key_violation(&self) -> bool {
        matches!(self, Self::Sql(error) if error.code() == Some("23503"))
    }
}

/// The pool, or a clear refusal - an in-memory `Db` would silently lose everything on restart.
pub const fn pool(db: &Db) -> Result<&Pool, StorageError> {
    match db {
        Db::Postgres(postgres) => Ok(postgres.pool()),
        Db::InMemory(_) => Err(StorageError::NotPostgres),
    }
}

#[must_use]
pub fn schema() -> String {
    quench_core::env::get_or("DB_SCHEMA", "warehouse")
}
