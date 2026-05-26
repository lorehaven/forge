use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use std::fmt::Debug;

pub mod backends;
pub mod error;
pub mod migrations;
pub mod prelude;

pub use backends::in_memory::{InMemoryDb, InMemoryRepository};
pub use backends::postgres::{PostgresDb, PostgresRepository};
pub use error::DbError;
pub use migrations::{ChangeSet, ColumnDef, Migration, MigrationFile, MigrationLoader};

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct TestModel {
        id: String,
        name: String,
    }

    impl Model for TestModel {
        fn table_name() -> String {
            "test_table".to_string()
        }

        fn columns() -> Vec<&'static str> {
            vec!["id", "name"]
        }
    }

    #[test]
    fn test_model_trait() {
        assert_eq!(TestModel::table_name(), "test_table");
    }
}
