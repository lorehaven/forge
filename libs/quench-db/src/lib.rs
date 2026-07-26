use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use std::fmt::Debug;

pub mod backends;
pub mod catalog;
pub mod error;
pub mod migrations;
pub mod plan;
pub mod prelude;
pub mod runner;

pub use backends::in_memory::{InMemoryDb, InMemoryRepository};
pub use backends::postgres::{PostgresDb, PostgresRepository};
pub use catalog::{Catalog, CatalogError, CatalogModule, ModuleManifest, ModuleScope, Requirement};
pub use error::DbError;
pub use migrations::{ChangeSet, ColumnDef, Migration, MigrationFile, MigrationLoader};
pub use plan::{InstallRequest, MigrationPlan, PlanError, PlannedMigration, ResolvedModule};
pub use runner::{ApplyReport, MigrationOutcome, MigrationResult, MigrationRunner, ResetReport};

#[async_trait]
pub trait Database: Send + Sync + Debug {
    async fn execute(&self, query: &str) -> Result<(), DbError>;
    async fn migrate(&self, migrations: Vec<Migration>) -> Result<(), DbError>;
}

#[async_trait]
pub trait Model: Serialize + DeserializeOwned + Send + Sync + Debug + Unpin + Clone {
    fn table_name() -> String;
    fn columns() -> Vec<&'static str>;
    fn primary_key_name() -> String {
        "id".to_string()
    }
}

#[async_trait]
pub trait Crud<T: Model>: Send + Sync {
    async fn create(&self, model: &T) -> Result<T, DbError>;
    async fn read(&self, id: &str) -> Result<Option<T>, DbError>;
    async fn update(&self, model: &T) -> Result<T, DbError>;
    async fn delete(&self, id: &str) -> Result<(), DbError>;
    async fn list(&self) -> Result<Vec<T>, DbError>;

    /// Rows where `column` equals `value`.
    ///
    /// The value binds as a parameter; the column cannot, so it is checked
    /// against [`Model::columns`] before being interpolated.
    async fn find_by(&self, column: &str, value: &str) -> Result<Vec<T>, DbError>;
}

/// Rejects a column that the model does not declare, so a caller cannot smuggle
/// SQL through the identifier position of `find_by`.
pub(crate) fn checked_column<T: Model>(column: &str) -> Result<&'static str, DbError> {
    T::columns()
        .into_iter()
        .find(|known| *known == column)
        .ok_or_else(|| {
            DbError::Unknown(format!(
                "unknown column '{column}' for table {}",
                T::table_name()
            ))
        })
}

#[derive(Clone, Debug)]
pub enum Db {
    Postgres(PostgresDb),
    InMemory(InMemoryDb),
}

impl Db {
    pub async fn connect(url: &str) -> Result<Self, DbError> {
        if url.is_empty() {
            Ok(Db::InMemory(InMemoryDb::new()))
        } else {
            Ok(Db::Postgres(PostgresDb::new(url).await?))
        }
    }

    pub fn repository<T: Model>(&self) -> Repository<T> {
        match self {
            Db::Postgres(db) => Repository::Postgres(PostgresRepository::new(db.clone())),
            Db::InMemory(db) => Repository::InMemory(InMemoryRepository::new(db.clone())),
        }
    }
}

#[async_trait]
impl Database for Db {
    async fn execute(&self, query: &str) -> Result<(), DbError> {
        match self {
            Db::Postgres(db) => db.execute(query).await,
            Db::InMemory(db) => db.execute(query).await,
        }
    }

    async fn migrate(&self, migrations: Vec<Migration>) -> Result<(), DbError> {
        match self {
            Db::Postgres(db) => db.migrate(migrations).await,
            Db::InMemory(db) => db.migrate(migrations).await,
        }
    }
}

pub enum Repository<T: Model> {
    Postgres(PostgresRepository<T>),
    InMemory(InMemoryRepository<T>),
}

#[async_trait]
impl<T> Crud<T> for Repository<T>
where
    T: Model + for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>,
{
    async fn create(&self, model: &T) -> Result<T, DbError> {
        match self {
            Repository::Postgres(repo) => repo.create(model).await,
            Repository::InMemory(repo) => repo.create(model).await,
        }
    }

    async fn read(&self, id: &str) -> Result<Option<T>, DbError> {
        match self {
            Repository::Postgres(repo) => repo.read(id).await,
            Repository::InMemory(repo) => repo.read(id).await,
        }
    }

    async fn update(&self, model: &T) -> Result<T, DbError> {
        match self {
            Repository::Postgres(repo) => repo.update(model).await,
            Repository::InMemory(repo) => repo.update(model).await,
        }
    }

    async fn delete(&self, id: &str) -> Result<(), DbError> {
        match self {
            Repository::Postgres(repo) => repo.delete(id).await,
            Repository::InMemory(repo) => repo.delete(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<T>, DbError> {
        match self {
            Repository::Postgres(repo) => repo.list().await,
            Repository::InMemory(repo) => repo.list().await,
        }
    }

    async fn find_by(&self, column: &str, value: &str) -> Result<Vec<T>, DbError> {
        match self {
            Repository::Postgres(repo) => repo.find_by(column, value).await,
            Repository::InMemory(repo) => repo.find_by(column, value).await,
        }
    }
}
