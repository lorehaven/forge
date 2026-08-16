//! Shared plumbing every domain module reads from: the pool, the schema name,
//! and the one error type raw SQL access here returns.
//!
//! Raw SQL rather than `quench-db`'s generic `Crud`, for the same reason
//! conveyor's scheduler is (`docker/conveyor-service/src/scheduler/queue.rs`):
//! issue creation needs a locked read-then-insert (see `issue::create`) that
//! `Crud` has no way to express, and half the schema going through one path
//! and half through another is worse than all of it going through this one.

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
    /// Whether this is the foreign-key violation Postgres raises when a row
    /// references something that doesn't exist - e.g. an issue's `assignee`
    /// naming a username that isn't in the realm.
    pub fn is_foreign_key_violation(&self) -> bool {
        matches!(self, Self::Sql(sqlx::Error::Database(database)) if database.code().as_deref() == Some("23503"))
    }

    /// Whether this is the unique-constraint violation Postgres raises when a
    /// row collides with one that already has the same identity - e.g. a
    /// project `key` that's already taken.
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::Sql(sqlx::Error::Database(database)) if database.code().as_deref() == Some("23505"))
    }
}

/// The pool, or a clear refusal.
///
/// The estate allows an in-memory database for tests. Issues and comments on
/// top of one would look like they worked and lose everything on restart, so
/// this says so rather than degrading quietly.
pub fn pool(db: &Db) -> Result<&Pool<Postgres>, WorkbenchError> {
    match db {
        Db::Postgres(postgres) => Ok(postgres.pool()),
        Db::InMemory(_) => Err(WorkbenchError::NotPostgres),
    }
}

pub fn schema() -> String {
    envmnt::get_or("DB_SCHEMA", "workbench")
}
