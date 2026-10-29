use std::sync::Arc;

use hoist_service::config::Settings;
use hoist_service::{refresh, routers};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_http::prelude::*;
use quench_starter::common::db::DbWrapper;
use quench_starter::common::health::HealthState;
use quench_starter::common::routes::normalize_base_path;
use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use quench_starter::http::serve_app;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    quench_log::info!("Hoist service starting");

    let settings = Settings::from_env();
    let base_path = normalize_base_path(&quench_core::env::get_or("BASE_PATH", "/"));
    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = JwtConfig::init().await;
    // `Auth` checks every token's session against the realm's Redis (REDIS_URL); a store of this service's own would
    // never see a session gatehouse just made or revoked.
    let session_db = SessionDb::from_env().await.expect("session store");
    let health_state = HealthState::live();
    let gatehouse = gatehouse_health_url();
    health_state.spawn_ready_when(async move {
        wait_for_services("hoist-service", vec![gatehouse.as_str()]).await;
    });

    // Ready does not wait for the first refresh: an Aptoide crawl should not hold up the probe.
    refresh::spawn_scheduler(db_wrapper.db.clone(), settings);

    let container = ContainerBuilder::new()
        .provide(db_wrapper.db.clone())
        .provide(health_state)
        .provide(jwt_config.clone())
        .provide_arc(session_db)
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    routers::register_routes();
    let app = quench_starter::http::discover_and_mount(base_path.clone());
    let app = routers::wrap_auth(app, jwt_config, &base_path);
    serve_app("hoist-service", app, container).await
}
