use quench_http::prelude::*;
use quench_starter::common::health::HealthState;
use quench_starter::common::routes::normalize_base_path;
use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use quench_starter::http::serve_app;
use sage_service::{routers, startup};
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    startup::init_tracing();
    envmnt::set("DB_SCHEMA", envmnt::get_or("DB_SCHEMA", "sage"));

    let base_path = normalize_base_path(&envmnt::get_or("BASE_PATH", "/"));
    tracing::info!("Server initialized with BASE_PATH: {base_path}");

    let switchboard_health_url = startup::health_check_url();
    let (state, db_wrapper) = startup::AppState::init().await;

    // Kept for graceful shutdown - the container below takes ownership of state's pieces.
    let shutdown_switchboard = state.switchboard.clone();
    let shutdown_config = state.config.clone();

    let init_switchboard = state.switchboard.clone();
    let init_config = state.config.clone();

    let launched = startup::default_models::LaunchedInstances::default();
    let init_launched = launched.clone();
    let shutdown_launched = launched;

    let health_state = HealthState::live();
    let gatehouse_health_url = gatehouse_health_url();
    health_state.spawn_ready_when(async move {
        wait_for_services(
            "sage-service",
            vec![
                switchboard_health_url.as_str(),
                gatehouse_health_url.as_str(),
            ],
        )
        .await;

        startup::default_models::spawn_monitor(
            init_switchboard.clone(),
            init_config.clone(),
            init_launched,
        );

        if let Err(e) = startup::validate_startup(&init_switchboard, &init_config).await {
            tracing::error!("Startup validation failed: {}", e);
            tracing::error!(
                "The service may not function correctly. Please check your configuration."
            );
        }
    });

    let jwt_config = state.jwt_config.clone();

    let container = ContainerBuilder::new()
        .provide(db_wrapper.db.clone())
        .provide(health_state)
        .provide(state.switchboard)
        .provide(state.vllm)
        .provide(state.config)
        .provide_arc(state.chat_state)
        .provide(state.jwt_config)
        .provide(state.sso_config)
        .provide_arc(state.user_db)
        .provide_arc(state.session_db)
        .provide_arc(state.tool_registry)
        .provide_arc(state.search_providers)
        .provide_arc(state.metrics)
        .provide_arc(state.rate_limiter)
        .provide_arc(state.cost_tracker)
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    // Only routes actually linked into the binary get discovered - see swagger::register_routes.
    routers::register_routes();

    let app = quench_starter::http::discover_and_mount(base_path.clone());
    let app = routers::wrap_auth(app, jwt_config, &base_path);

    let result = serve_app("sage-service", app, container).await;

    // Runs after graceful shutdown, to tear down default models launched at startup.
    startup::default_models::shutdown(&shutdown_switchboard, &shutdown_config, &shutdown_launched)
        .await;

    result
}
