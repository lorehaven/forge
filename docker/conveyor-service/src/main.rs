use conveyor_service::{routers, startup};
use quench_http::prelude::*;
use quench_starter::common::health::HealthState;
use quench_starter::common::routes::normalize_base_path;
use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use quench_starter::http::serve_app;
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    startup::init_tracing();
    envmnt::set("SERVICE_NAME", envmnt::get_or("SERVICE_NAME", "conveyor"));
    envmnt::set("DB_SCHEMA", envmnt::get_or("DB_SCHEMA", "conveyor"));

    // Written up front so an early stylesheet request doesn't hit the last deployment's file.
    routers::ui::common::ensure_assets();

    let base_path = normalize_base_path(&envmnt::get_or("BASE_PATH", "/"));
    tracing::info!("Server initialized with BASE_PATH: {base_path}");

    let (state, db_wrapper) = startup::AppState::init().await;

    // Reported at startup, not discovered mid-first-run.
    startup::report_toolchain(&state.config);

    // Workers share this process but talk to it only via the DB, so serving and building scale independently.
    conveyor_service::scheduler::spawn_pool(
        state.db.clone(),
        state.config.clone(),
        state.executor.0.clone(),
        state.providers.clone(),
    );

    let health_state = HealthState::live();
    let gatehouse_health_url = gatehouse_health_url();
    health_state.spawn_ready_when(async move {
        wait_for_services("conveyor-service", vec![gatehouse_health_url.as_str()]).await;
    });

    let jwt_config = state.jwt_config.clone();

    let container = ContainerBuilder::new()
        .provide(db_wrapper.db.clone())
        .provide(health_state)
        .provide(state.config)
        .provide(state.jwt_config)
        .provide(state.sso_config)
        .provide_arc(state.user_db)
        .provide_arc(state.session_db)
        .provide(state.db)
        .provide(state.executor)
        .provide_arc(state.providers)
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    // Only linked-in routes get discovered - see `routers::swagger::register_routes`'s doc comment.
    routers::register_routes();

    let app = quench_starter::http::discover_and_mount(base_path.clone());
    let app = routers::wrap_auth(app, jwt_config, &base_path);

    serve_app("conveyor-service", app, container).await
}
