use quench_starter::prelude::*;
use sage_service::{base_path_scope, root_scope, startup};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    startup::init_tracing();
    envmnt::set("DB_SCHEMA", envmnt::get_or("DB_SCHEMA", "sage"));

    let health_url = startup::health_check_url();
    let (state, db_wrapper) = startup::AppState::init().await;

    startup::default_models::spawn_monitor(state.switchboard.clone(), state.config.clone());

    // Clones retained for graceful shutdown after the server stops; `state` is
    // moved into the server factory closure below.
    let shutdown_switchboard = state.switchboard.clone();
    let shutdown_config = state.config.clone();

    // Validate critical dependencies at startup
    if let Err(e) = startup::validate_startup(&state.switchboard, &state.config).await {
        tracing::error!("Startup validation failed: {}", e);
        tracing::error!("The service may not function correctly. Please check your configuration.");
    }

    let result = serve(
        root_scope,
        move || base_path_scope(state.clone()),
        Some(db_wrapper),
        async move {
            wait_for_services("sage-service", vec![health_url.as_str()]).await;
        },
    )
    .await;

    // The server future resolves once actix has completed its graceful
    // shutdown (e.g. on SIGTERM/SIGINT), so the default models we asked
    // switchboard to launch at startup can be torn down here.
    startup::default_models::shutdown(&shutdown_switchboard, &shutdown_config).await;

    result
}
