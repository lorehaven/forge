use quench_auth::http::domain::sso_client::SsoConfig;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_starter::prelude::DbWrapper;
use std::sync::Arc;

use crate::config::ConveyorConfig;
use crate::executors::Executor;
use crate::providers::Providers;

/// Everything the HTTP layer needs, built once at startup and provided into
/// the DI container in `main.rs`.
#[derive(Clone)]
pub struct AppState {
    pub config: ConveyorConfig,
    pub jwt_config: JwtConfig,
    pub sso_config: SsoConfig,
    pub user_db: Arc<UserDb>,
    pub session_db: Arc<SessionDb>,
    pub db: quench_db::prelude::Db,
    /// Shared, not one per worker: a job started by the request that triggered
    /// it has to be pollable by every other request.
    pub executor: Executor,
    /// Also shared: each provider holds an HTTP client, and one per request
    /// would throw away every pooled connection.
    pub providers: Arc<Providers>,
}

impl AppState {
    /// Builds every shared value; returned `DbWrapper` goes to `serve` for health reporting.
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
            jwt_config: JwtConfig::init().await,
            sso_config: SsoConfig::init(),
            user_db: UserDb::init(db_wrapper.db.clone()).await,
            // Sessions live in the shared store, so a logout at gatehouse is
            // immediately a logout here.
            session_db: SessionDb::from_env()
                .await
                .expect("session store unavailable"),
            db: db_wrapper.db.clone(),
            executor: Executor(crate::executors::build(config.executor).await),
            providers: Arc::new(Providers::from_env()),
            config,
        };

        (state, db_wrapper)
    }
}
