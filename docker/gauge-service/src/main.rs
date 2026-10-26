use gauge_service::domain::{collector, config::Config, store::Store};
use gauge_service::routers;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_auth::http::domain::sso_client::SsoConfig;
use quench_http::prelude::*;
use quench_starter::common::routes::normalize_base_path;
use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use quench_starter::http::serve_app;
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    quench_log::info!("Gauge service starting");

    let base_path = normalize_base_path(&quench_core::env::get_or("BASE_PATH", "/"));
    quench_log::info!("Server initialized with BASE_PATH: {base_path}");

    let config = Config::load().unwrap_or_else(|reason| panic!("configuration: {reason}"));
    quench_log::info!(
        "watching {} host(s) every {}s",
        config.targets.len(),
        config.interval_secs
    );
    let store = Store::new(&config.targets);
    collector::spawn(&config, &store);

    let jwt_config = JwtConfig::init().await;
    let sso_config = SsoConfig::init();
    let session_db = SessionDb::from_env().await.expect("session store");

    let health_state = quench_starter::common::health::HealthState::live();
    let gatehouse_health_url = gatehouse_health_url();
    health_state.spawn_ready_when(async move {
        wait_for_services("gauge-service", vec![gatehouse_health_url.as_str()]).await;
    });

    let container = ContainerBuilder::new()
        .provide(store)
        .provide(health_state)
        .provide(jwt_config.clone())
        .provide(sso_config)
        .provide_arc(session_db)
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    routers::api::register_routes();
    routers::ui::register_routes();

    let app = quench_starter::http::discover_and_mount(base_path.clone());
    let app = routers::api::wrap_auth(app, jwt_config, &base_path);

    serve_app("gauge-service", app, container).await
}
