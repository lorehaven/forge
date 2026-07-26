use crate::{Crud, Database, DbError, Migration, Model};
use async_trait::async_trait;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use std::fmt::Debug;
use tracing;

#[derive(Clone)]
pub struct PostgresDb {
    pool: Pool<Postgres>,
}

impl PostgresDb {
    pub async fn new(url: &str) -> Result<Self, DbError> {
        // Install default crypto provider for rustls 0.23+
        let _ = rustls::crypto::ring::default_provider().install_default();

        let max_connections = std::env::var("DB_POOL_MAX_SIZE")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .unwrap_or(5);

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    fn migration_table() -> String {
        std::env::var("DB_MIGRATION_TABLE").unwrap_or_else(|_| "quench_migrations".to_string())
    }

    async fn ensure_migration_table(&self) -> Result<(), DbError> {
        let table = Self::migration_table();
        let query = format!(
            "
            CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                author TEXT NOT NULL,
                applied_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
            )
        ",
            table
        );
        self.execute(&query).await
    }
}

impl Debug for PostgresDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresDb").finish()
    }
}

#[async_trait]
impl Database for PostgresDb {
    async fn execute(&self, query: &str) -> Result<(), DbError> {
        sqlx::query(sqlx::AssertSqlSafe(query))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn migrate(&self, migrations: Vec<Migration>) -> Result<(), DbError> {
        self.ensure_migration_table().await?;
        let table = Self::migration_table();

        for migration in migrations {
            // Check if migration already applied
            let query = format!("SELECT EXISTS(SELECT 1 FROM {} WHERE id = $1)", table);
            let exists: (bool,) = sqlx::query_as(sqlx::AssertSqlSafe(query.as_str()))
                .bind(&migration.id)
                .fetch_one(&self.pool)
                .await?;

            if !exists.0 {
                tracing::info!("Applying migration: {}", migration.id);
                let mut tx = self.pool.begin().await?;

                for change in migration.changes {
                    let sql = change.to_sql();
                    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                        .execute(&mut *tx)
                        .await?;
                }

                let insert_query = format!("INSERT INTO {} (id, author) VALUES ($1, $2)", table);
                sqlx::query(sqlx::AssertSqlSafe(insert_query.as_str()))
                    .bind(&migration.id)
                    .bind(&migration.author)
                    .execute(&mut *tx)
                    .await?;

                tx.commit().await?;
            }
        }

        Ok(())
    }
}

pub struct PostgresRepository<T: Model> {
    db: PostgresDb,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Model> PostgresRepository<T> {
    pub fn new(db: PostgresDb) -> Self {
        Self {
            db,
            _marker: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<T> Crud<T> for PostgresRepository<T>
where
    T: Model + for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>,
{
    async fn create(&self, model: &T) -> Result<T, DbError> {
        let table = T::table_name();
        let json = serde_json::to_value(model)?;
        let query = format!(
            "INSERT INTO {} SELECT * FROM jsonb_populate_record(NULL::{}, $1) RETURNING *",
            table, table
        );

        let result = sqlx::query_as::<_, T>(sqlx::AssertSqlSafe(query.as_str()))
            .bind(json)
            .fetch_one(&self.db.pool)
            .await?;
        Ok(result)
    }

    async fn read(&self, id: &str) -> Result<Option<T>, DbError> {
        let query = format!(
            "SELECT * FROM {} WHERE {} = $1",
            T::table_name(),
            T::primary_key_name()
        );
        let row = sqlx::query_as::<_, T>(sqlx::AssertSqlSafe(query.as_str()))
            .bind(id)
            .fetch_optional(&self.db.pool)
            .await?;
        Ok(row)
    }

    async fn update(&self, model: &T) -> Result<T, DbError> {
        let table = T::table_name();
        let pk = T::primary_key_name();
        let columns = T::columns();
        let json = serde_json::to_value(model)?;

        let cols_list = columns.join(", ");

        let query = format!(
            "UPDATE {table} SET ({cols}) = (SELECT {cols} FROM jsonb_populate_record(NULL::{table}, $1)) WHERE {pk} = $2 RETURNING *",
            table = table,
            cols = cols_list,
            pk = pk
        );

        let id_val = json
            .get(&pk)
            .ok_or_else(|| DbError::Unknown(format!("Primary key '{}' not found in model", pk)))?;

        let id = match id_val {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => {
                return Err(DbError::Unknown(format!(
                    "Primary key '{}' is not a string or number",
                    pk
                )));
            }
        };

        let result = sqlx::query_as::<_, T>(sqlx::AssertSqlSafe(query.as_str()))
            .bind(json)
            .bind(id)
            .fetch_one(&self.db.pool)
            .await?;
        Ok(result)
    }

    async fn delete(&self, id: &str) -> Result<(), DbError> {
        let query = format!(
            "DELETE FROM {} WHERE {} = $1",
            T::table_name(),
            T::primary_key_name()
        );
        sqlx::query(sqlx::AssertSqlSafe(query.as_str()))
            .bind(id)
            .execute(&self.db.pool)
            .await?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<T>, DbError> {
        let query = format!("SELECT * FROM {}", T::table_name());
        let rows = sqlx::query_as::<_, T>(sqlx::AssertSqlSafe(query.as_str()))
            .fetch_all(&self.db.pool)
            .await?;
        Ok(rows)
    }

    async fn find_by(&self, column: &str, value: &str) -> Result<Vec<T>, DbError> {
        let column = crate::checked_column::<T>(column)?;
        let query = format!("SELECT * FROM {} WHERE {} = $1", T::table_name(), column);
        let rows = sqlx::query_as::<_, T>(sqlx::AssertSqlSafe(query.as_str()))
            .bind(value)
            .fetch_all(&self.db.pool)
            .await?;
        Ok(rows)
    }
}
