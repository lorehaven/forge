use quench_starter::prelude::*;
use sage_service::{base_path_scope, root_scope, startup};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    startup::init_tracing();
    envmnt::set("DB_SCHEMA", envmnt::get_or("DB_SCHEMA", "sage"));

    let switchboard_health_url = startup::health_check_url();
    let (state, db_wrapper) = startup::AppState::init().await;

    // Clones retained for graceful shutdown, since `state` moves into the server factory closure below.
    let shutdown_switchboard = state.switchboard.clone();
    let shutdown_config = state.config.clone();

    let init_switchboard = state.switchboard.clone();
    let init_config = state.config.clone();

    let launched = startup::default_models::LaunchedInstances::default();
    let init_launched = launched.clone();
    let shutdown_launched = launched;

    let result = serve(
        root_scope,
        move || base_path_scope(state.clone()),
        Some(db_wrapper),
        async move {
            wait_for_services(
                "sage-service",
                vec![
                    switchboard_health_url.as_str(),
                    gatehouse_health_url().as_str(),
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
        },
    )
    .await;

    // Runs after actix's graceful shutdown (SIGTERM/SIGINT) resolves, so the default models launched at startup can be torn down here.
    startup::default_models::shutdown(&shutdown_switchboard, &shutdown_config, &shutdown_launched)
        .await;

    result
}
