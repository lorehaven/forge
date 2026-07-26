use quench_cli::prelude::{Tone, print_status};
use quench_db::prelude::Db;
use std::sync::Arc;

pub struct DbWrapper {
    pub db: Db,
}

impl DbWrapper {
    pub async fn init_env() -> Arc<Self> {
        let db_url = envmnt::get_any(&vec!["DATABASE_URL", "POSTGRES_URL"], "");
        Self::init(db_url).await
    }

    /// Connects, or refuses to start.
    ///
    /// Schema lifecycle belongs to foundry: this no longer migrates at boot and
    /// no longer has a recreate switch, so a service can neither create nor
    /// destroy the schema it runs against.
    pub async fn init(url: impl ToString) -> Arc<Self> {
        let db_url = url.to_string();

        // An empty URL silently produces an in-memory database. A service that
        // starts that way looks healthy, serves requests, and loses everything
        // on restart - so it has to be asked for explicitly.
        if db_url.trim().is_empty() && !envmnt::is_or("ALLOW_IN_MEMORY_DB", false) {
            panic!(
                "no database configured: set DATABASE_URL (or POSTGRES_URL). \
                 Set ALLOW_IN_MEMORY_DB=true only for tests, where losing every \
                 write on restart is the intent."
            );
        }

        let db = match Db::connect(&db_url).await {
            Ok(db) => {
                match &db {
                    Db::Postgres(_) => print_status(Tone::Success, "db", "connected to Postgres"),
                    Db::InMemory(_) => print_status(
                        Tone::Warn,
                        "db",
                        "ALLOW_IN_MEMORY_DB is set: data lives in memory and is lost on restart",
                    ),
                }
                db
            }
            Err(e) => panic!("configured database connection failed: {e}"),
        };

        Arc::new(Self { db })
    }
}
