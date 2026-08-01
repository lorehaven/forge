use conveyor_service::{base_path_scope, root_scope, routers, startup};
use quench_starter::prelude::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    startup::init_tracing();
    envmnt::set("SERVICE_NAME", envmnt::get_or("SERVICE_NAME", "conveyor"));
    envmnt::set("DB_SCHEMA", envmnt::get_or("DB_SCHEMA", "conveyor"));

    // Written before the first request: left to the first page render, a
    // request for the stylesheet that arrives earlier is answered from whatever
    // the previous deployment left on disk.
    routers::ui::common::ensure_assets();

    let (state, db_wrapper) = startup::AppState::init().await;

    // Reported at startup rather than discovered three minutes into somebody's
    // first run.
    startup::report_toolchain(&state.config);

    // The workers share this process with the HTTP server. They talk to it only
    // through the database, so a replica that serves no traffic still builds,
    // and one that builds nothing still serves.
    conveyor_service::scheduler::spawn_pool(
        state.db.clone(),
        state.config.clone(),
        state.executor.clone(),
        state.providers.clone(),
    );

    serve(
        root_scope,
        move || base_path_scope(state.clone()),
        Some(db_wrapper),
        async move {
            wait_for_services("conveyor-service", vec![gatehouse_health_url().as_str()]).await;
        },
    )
    .await
}
