//! Shared pool/schema/error plumbing: raw SQL, not `Crud` - claiming the next queued operation needs a
//! locked read-then-update that `Crud` cannot express.

use quench_db::prelude::Db;
use quench_pg::Pool;

#[derive(Debug, quench_core::Error)]
pub enum GantryError {
    #[error(
        "gantry needs Postgres; this service is running against an in-memory database, \
         where every operation and its history would be lost on restart"
    )]
    NotPostgres,

    #[error(transparent)]
    Sql(#[from] quench_pg::Error),

    #[error("a stored plan could not be read: {0}")]
    Plan(#[from] quench_serde::Error),
}

impl GantryError {
    /// Postgres's unique-constraint violation, e.g. a second running operation in one scope.
    #[must_use]
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::Sql(error) if error.code() == Some("23505"))
    }
}

/// The pool, or a clear refusal - an in-memory `Db` would silently lose data.
pub const fn pool(db: &Db) -> Result<&Pool, GantryError> {
    match db {
        Db::Postgres(postgres) => Ok(postgres.pool()),
        Db::InMemory(_) => Err(GantryError::NotPostgres),
    }
}

#[must_use]
pub fn schema() -> String {
    quench_core::env::get_or("DB_SCHEMA", "gantry")
}
