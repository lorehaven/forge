use quench_starter::prelude::*;
use sage_service::{base_path_scope, root_scope, startup};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    startup::init_tracing();
    envmnt::set("DB_SCHEMA", envmnt::get_or("DB_SCHEMA", "sage"));

    let switchboard_health_url = startup::health_check_url();
    let (state, db_wrapper) = startup::AppState::init().await;

    // Clones retained for graceful shutdown after the server stops; `state` is
    // moved into the server factory closure below.
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
            // Switchboard and gatehouse are both hard dependencies - the
            // default-model monitor and the startup connectivity check below
            // both call switchboard, and every request sage itself serves
            // verifies against gatehouse's JWKS. Waiting here, instead of
            // firing those checks the instant the process starts, is what
            // stops them from racing a dependency's own rollout (as happened
            // when sage's first attempts landed before switchboard's new pod
            // had passed readiness) and burning through their retry budgets
            // on a dependency that was never actually down.
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

    // The server future resolves once actix has completed its graceful
    // shutdown (e.g. on SIGTERM/SIGINT), so the default models we asked
    // switchboard to launch at startup can be torn down here.
    startup::default_models::shutdown(&shutdown_switchboard, &shutdown_config, &shutdown_launched)
        .await;

    result
}
