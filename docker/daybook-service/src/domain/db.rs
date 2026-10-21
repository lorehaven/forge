use quench_db::prelude::Db;
use sqlx::{Pool, Postgres};

#[derive(Debug, thiserror::Error)]
pub enum DaybookError {
    #[error("daybook requires Postgres; the in-memory database does not persist tasks")]
    NotPostgres,
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

pub const fn pool(db: &Db) -> Result<&Pool<Postgres>, DaybookError> {
    match db {
        Db::Postgres(postgres) => Ok(postgres.pool()),
        Db::InMemory(_) => Err(DaybookError::NotPostgres),
    }
}

/// Database schema configured by the service's migration module.
#[must_use]
pub fn schema() -> String {
    envmnt::get_or("DB_SCHEMA", "daybook")
}
