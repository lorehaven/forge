use quench_cli::prelude::{Tone, print_status};
use quench_db::prelude::{Database, Db, MigrationLoader};
use std::sync::Arc;

pub struct DbWrapper {
    pub db: Db,
}

impl DbWrapper {
    pub async fn init_env() -> Arc<Self> {
        let db_url = envmnt::get_any(&vec!["DATABASE_URL", "POSTGRES_URL"], "");
        Self::init(db_url).await
    }

    pub async fn init(url: impl ToString) -> Arc<Self> {
        let db_url = url.to_string();

        let db = match Db::connect(&db_url).await {
            Ok(db) => {
                match &db {
                    Db::Postgres(_) => print_status(Tone::Success, "db", "connected to Postgres"),
                    Db::InMemory(_) => print_status(
                        Tone::Info,
                        "db",
                        "DATABASE_URL not set, using in-memory database",
                    ),
                }

                let schema = envmnt::get_or("DB_SCHEMA", "public");
                let recreate = envmnt::is_or("DB_RECREATE", false);

                if recreate && let Db::Postgres(pg_db) = &db {
                    print_status(
                        Tone::Warn,
                        "db",
                        &format!(
                            "DB_RECREATE=true, dropping schema {} and migration table",
                            schema
                        ),
                    );
                    if schema != "public" {
                        pg_db
                            .execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", schema))
                            .await
                            .ok();
                    }
                    let migration_table = envmnt::get_or("DB_MIGRATION_TABLE", "quench_migrations");
                    pg_db
                        .execute(&format!("DROP TABLE IF EXISTS {}", migration_table))
                        .await
                        .ok();
                }

                // Load migrations from directory
                let migrations_dir = envmnt::get_or("MIGRATIONS_DIR", "migrations");
                if std::path::Path::new(&migrations_dir).exists() {
                    match MigrationLoader::load_from_dir(&migrations_dir) {
                        Ok(migrations) => {
                            if let Err(e) = db.migrate(migrations).await {
                                panic!("database migration failed: {e}");
                            }
                        }
                        Err(e) => {
                            panic!("failed to load migrations from {migrations_dir}: {e}");
                        }
                    }
                }

                db
            }
            Err(e) => {
                panic!("configured database connection failed: {e}");
            }
        };

        Arc::new(Self { db })
    }
}
