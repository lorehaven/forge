use crate::{Crud, Database, DbError, Migration, Model};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};

type TableMap = HashMap<String, Arc<RwLock<HashMap<String, serde_json::Value>>>>;

#[derive(Clone, Default)]
pub struct InMemoryDb {
    tables: Arc<RwLock<TableMap>>,
}

impl InMemoryDb {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_table(&self, name: &str) -> Arc<RwLock<HashMap<String, serde_json::Value>>> {
        let mut tables = self.tables.write().unwrap();
        tables
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(HashMap::new())))
            .clone()
    }
}

impl Debug for InMemoryDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryDb").finish()
    }
}

#[async_trait]
impl Database for InMemoryDb {
    async fn execute(&self, _query: &str) -> Result<(), DbError> {
        // SQL execution not supported for in-memory, but we ignore it for migrations
        Ok(())
    }

    async fn migrate(&self, _migrations: Vec<Migration>) -> Result<(), DbError> {
        // Migrations are essentially NO-OPs or can be used to pre-seed
        Ok(())
    }
}

pub struct InMemoryRepository<T: Model> {
    db: InMemoryDb,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Model> InMemoryRepository<T> {
    pub fn new(db: InMemoryDb) -> Self {
        Self {
            db,
            _marker: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<T> Crud<T> for InMemoryRepository<T>
where
    T: Model,
{
    async fn create(&self, model: &T) -> Result<T, DbError> {
        let table = self.db.get_table(&T::table_name());
        let mut data = table.write().unwrap();
        let json = serde_json::to_value(model)?;
        let id = json
            .get(T::primary_key_name())
            .ok_or_else(|| DbError::Unknown("Missing primary key".to_string()))?
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| {
                json.get(T::primary_key_name())
                    .and_then(|v| v.as_i64())
                    .map(|n| n.to_string())
            })
            .ok_or_else(|| DbError::Unknown("Primary key must be string or number".to_string()))?;

        data.insert(id, json);
        Ok(model.clone())
    }

    async fn read(&self, id: &str) -> Result<Option<T>, DbError> {
        let table = self.db.get_table(&T::table_name());
        let data = table.read().unwrap();
        if let Some(json) = data.get(id) {
            let model = serde_json::from_value(json.clone())?;
            Ok(Some(model))
        } else {
            Ok(None)
        }
    }

    async fn update(&self, model: &T) -> Result<T, DbError> {
        self.create(model).await
    }

    async fn delete(&self, id: &str) -> Result<(), DbError> {
        let table = self.db.get_table(&T::table_name());
        let mut data = table.write().unwrap();
        data.remove(id);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<T>, DbError> {
        let table = self.db.get_table(&T::table_name());
        let data = table.read().unwrap();
        let mut results = Vec::new();
        for json in data.values() {
            results.push(serde_json::from_value(json.clone())?);
        }
        Ok(results)
    }
}
