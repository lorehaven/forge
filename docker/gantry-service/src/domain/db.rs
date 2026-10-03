//! Shared pool/schema/error plumbing: raw SQL, not `Crud` - claiming the next queued operation needs a
//! locked read-then-update that `Crud` cannot express.

use quench_db::prelude::Db;
use sqlx::{Pool, Postgres};

#[derive(Debug, thiserror::Error)]
pub enum GantryError {
    #[error(
        "gantry needs Postgres; this service is running against an in-memory database, \
         where every operation and its history would be lost on restart"
    )]
    NotPostgres,

    #[error(transparent)]
    Sql(#[from] sqlx::Error),

    #[error("a stored plan could not be read: {0}")]
    Plan(#[from] serde_json::Error),
}

impl GantryError {
    /// Postgres's unique-constraint violation, e.g. a second running operation in one scope.
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::Sql(sqlx::Error::Database(database)) if database.code().as_deref() == Some("23505"))
    }
}

/// The pool, or a clear refusal - an in-memory `Db` would silently lose data.
pub fn pool(db: &Db) -> Result<&Pool<Postgres>, GantryError> {
    match db {
        Db::Postgres(postgres) => Ok(postgres.pool()),
        Db::InMemory(_) => Err(GantryError::NotPostgres),
    }
}

pub fn schema() -> String {
    envmnt::get_or("DB_SCHEMA", "gantry")
}
