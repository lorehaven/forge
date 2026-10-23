use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_http::prelude::*;
use quench_starter::common::db::DbWrapper;
use quench_starter::common::health::HealthState;
use quench_starter::common::routes::normalize_base_path;
use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use quench_starter::http::serve_app;
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    quench_log::info!("Daybook service starting");

    let base_path = normalize_base_path(&quench_core::env::get_or("BASE_PATH", "/"));
    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = JwtConfig::init().await;
    // `Auth` checks every token's session against this store (Redis, via REDIS_URL); without it in the
    // container each request is rejected as an inactive session.
    let session_db = SessionDb::from_env().await.expect("session store");
    let health_state = HealthState::live();
    let gatehouse = gatehouse_health_url();
    health_state.spawn_ready_when(async move {
        wait_for_services("daybook-service", vec![gatehouse.as_str()]).await;
    });

    let container = ContainerBuilder::new()
        .provide(db_wrapper.db.clone())
        .provide(health_state)
        .provide(jwt_config.clone())
        .provide_arc(session_db)
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    daybook_service::routers::register_routes();
    let app = quench_starter::http::discover_and_mount(base_path.clone());
    let app = daybook_service::routers::wrap_auth(app, jwt_config, &base_path);
    serve_app("daybook-service", app, container).await
}
