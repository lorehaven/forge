//! Shared pool/schema/error plumbing: raw SQL, not `Crud` - issue creation
//! needs a locked read-then-insert `Crud` can't express.

use quench_db::prelude::Db;
use sqlx::{Pool, Postgres};

#[derive(Debug, thiserror::Error)]
pub enum WorkbenchError {
    #[error(
        "workbench needs Postgres; this service is running against an \
         in-memory database, where every project and issue would be lost on restart"
    )]
    NotPostgres,

    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

impl WorkbenchError {
    /// Postgres's foreign-key violation, e.g. an `assignee` not in the realm.
    pub fn is_foreign_key_violation(&self) -> bool {
        matches!(self, Self::Sql(sqlx::Error::Database(database)) if database.code().as_deref() == Some("23503"))
    }

    /// Postgres's unique-constraint violation, e.g. a `key` already taken.
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::Sql(sqlx::Error::Database(database)) if database.code().as_deref() == Some("23505"))
    }
}

/// The pool, or a clear refusal - an in-memory `Db` would silently lose data.
pub fn pool(db: &Db) -> Result<&Pool<Postgres>, WorkbenchError> {
    match db {
        Db::Postgres(postgres) => Ok(postgres.pool()),
        Db::InMemory(_) => Err(WorkbenchError::NotPostgres),
    }
}

pub fn schema() -> String {
    envmnt::get_or("DB_SCHEMA", "workbench")
}
