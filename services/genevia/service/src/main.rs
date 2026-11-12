use genevia_service::{routers, store};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_auth::http::domain::sso_client::SsoConfig;
use quench_http::extract::BodyLimit;
use quench_http::prelude::*;
use quench_starter::common::db::DbWrapper;
use quench_starter::common::routes::normalize_base_path;
use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use quench_starter::http::serve_app;
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    quench_log::info!("Genevia starting");

    let base_path = normalize_base_path(&quench_core::env::get_or("BASE_PATH", "/"));
    quench_log::info!("Server initialized with BASE_PATH: {base_path}");

    // The schema belongs to Foundry (`foundry-service --install genevia apply`): the service connects, or refuses to start.
    let db = DbWrapper::init_env().await;
    store::init(db.db.clone()).unwrap_or_else(|e| panic!("{e}"));

    let jwt_config = JwtConfig::init().await;
    let sso_config = SsoConfig::init();
    let session_db = SessionDb::from_env().await.expect("session store");

    let health_state = quench_starter::common::health::HealthState::live();
    let gatehouse_health_url = gatehouse_health_url();
    health_state.spawn_ready_when(async move {
        wait_for_services("genevia-service", vec![gatehouse_health_url.as_str()]).await;
    });

    let container = ContainerBuilder::new()
        .provide(health_state)
        .provide(jwt_config.clone())
        .provide(sso_config)
        .provide_arc(session_db)
        .provide(BodyLimit(genevia_service::import::MAX_UPLOAD))
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    routers::api::register_routes();
    routers::ui::register_routes();

    let app = quench_starter::http::discover_and_mount(base_path.clone());
    let app = routers::api::wrap_auth(app, jwt_config, &base_path);

    serve_app("genevia-service", app, container).await
}
