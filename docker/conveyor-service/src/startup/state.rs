use actix_web::Scope;
use actix_web::web::Data;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_starter::prelude::DbWrapper;
use std::sync::Arc;

use crate::config::ConveyorConfig;
use crate::executors::JobExecutor;
use crate::providers::Providers;

/// Everything the HTTP layer needs, built once at startup and cloned into each
/// actix worker.
#[derive(Clone)]
pub struct AppState {
    pub config: ConveyorConfig,
    pub jwt_config: Data<JwtConfig>,
    pub user_db: Arc<UserDb>,
    pub session_db: Arc<SessionDb>,
    pub db: quench_db::prelude::Db,
    /// Shared, not one per worker: a job started by the request that triggered
    /// it has to be pollable by every other request.
    pub executor: Arc<dyn JobExecutor>,
    /// Also shared: each provider holds an HTTP client, and one per request
    /// would throw away every pooled connection.
    pub providers: Arc<Providers>,
}

impl AppState {
    /// Build every shared value the service needs. The returned `DbWrapper` is
    /// handed to `serve`, which owns health reporting; schema lifecycle belongs
    /// to foundry, not here.
    pub async fn init() -> (Self, Arc<DbWrapper>) {
        let db_wrapper = DbWrapper::init_env().await;
        let config = ConveyorConfig::load();

        tracing::info!(
            "conveyor configured: executor {}, work dir {}, {} concurrent run(s)",
            config.executor,
            config.work_dir.display(),
            config.max_concurrent_runs,
        );

        let state = Self {
            jwt_config: Data::new(JwtConfig::init()),
            user_db: UserDb::init(db_wrapper.db.clone()).await,
            // Sessions live in the shared store, so a logout at gatehouse is
            // immediately a logout here.
            session_db: SessionDb::from_env()
                .await
                .expect("session store unavailable"),
            db: db_wrapper.db.clone(),
            executor: crate::executors::build(config.executor).await,
            providers: Arc::new(Providers::from_env()),
            config,
        };

        (state, db_wrapper)
    }

    /// Attach every shared value to `scope` as actix app data.
    pub fn install(self, scope: Scope) -> Scope {
        scope
            .app_data(Data::new(self.config))
            .app_data(self.jwt_config)
            .app_data(Data::new(self.user_db))
            .app_data(Data::new(self.session_db))
            .app_data(Data::new(self.db))
            .app_data(Data::new(self.executor))
            .app_data(Data::from(self.providers))
    }
}
