use quench_core::Error;
use quench_db::prelude::Db;
use quench_pg::Pool;

#[derive(Debug, Error)]
pub enum DaybookError {
    #[error("daybook requires Postgres; the in-memory database does not persist tasks")]
    NotPostgres,
    #[error(transparent)]
    Sql(#[from] quench_pg::Error),
}

pub const fn pool(db: &Db) -> Result<&Pool, DaybookError> {
    match db {
        Db::Postgres(postgres) => Ok(postgres.pool()),
        Db::InMemory(_) => Err(DaybookError::NotPostgres),
    }
}

/// Database schema configured by the service's migration module.
#[must_use]
pub fn schema() -> String {
    quench_core::env::get_or("DB_SCHEMA", "daybook")
}
