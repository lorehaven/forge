use gantry_service::domain::operation::PgStore;
use gantry_service::domain::service::Gantry;
use gantry_service::domain::settings::Settings;
use gantry_service::domain::{cluster, executor, reconciler, registry};
use gantry_service::routers;
use quench_auth::domain::auth::UserDb;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_auth::http::domain::sso_client::SsoConfig;
use quench_http::prelude::*;
use quench_starter::common::db::DbWrapper;
use quench_starter::common::routes::normalize_base_path;
use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use quench_starter::http::serve_app;
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    tracing::info!("Gantry service starting");

    let base_path = normalize_base_path(&envmnt::get_or("BASE_PATH", "/"));
    tracing::info!("Server initialized with BASE_PATH: {base_path}");

    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = JwtConfig::init().await;
    let sso_config = SsoConfig::init();
    let user_db = UserDb::init(db_wrapper.db.clone()).await;
    let session_db = SessionDb::from_env().await.expect("session store");

    let health_state = quench_starter::common::health::HealthState::live();
    let gatehouse_health_url = gatehouse_health_url();
    health_state.spawn_ready_when(async move {
        wait_for_services("gantry-service", vec![gatehouse_health_url.as_str()]).await;
    });

    let settings = Settings::from_env().unwrap_or_else(|reason| panic!("configuration: {reason}"));
    tracing::info!(
        "cluster: {:?}, runner: {:?}, namespaces: {:?}",
        settings.cluster,
        settings.runner,
        settings.allowed_namespaces
    );
    let connected = cluster::connect(&settings)
        .await
        .unwrap_or_else(|e| panic!("cluster: {e}"));
    let cluster = connected.cluster.clone();
    let fake = connected.fake.clone();
    let registry = registry::build(&settings);
    let executor = executor::build(
        &settings,
        cluster.clone(),
        connected.fake.clone(),
        registry.clone(),
    );
    let gantry = Arc::new(Gantry::new(
        Arc::new(PgStore::new(db_wrapper.db.clone())),
        cluster,
        executor,
        registry,
        settings,
    ));
    // A simulated cluster starts out as if everything in it had been installed: record what each package
    // declares, as an install would have.
    if let Some(fake) = &fake {
        for (package, version) in fake.seeded_packages() {
            let items = fake.catalog(&package);
            if let (Some(version), false) = (version, items.is_empty()) {
                let _ = gantry
                    .store
                    .set_inventory(
                        &package,
                        &gantry_service::domain::operation::Inventory {
                            version,
                            resources: items,
                        },
                    )
                    .await;
            }
        }
    }

    // Picks up any operation a previous run left behind, then keeps going.
    tokio::spawn(reconciler::run(gantry.clone()));

    let container = ContainerBuilder::new()
        .provide_arc(gantry)
        .provide(db_wrapper.db.clone())
        .provide(health_state)
        .provide(jwt_config.clone())
        .provide(sso_config)
        .provide_arc(user_db)
        .provide_arc(session_db)
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    routers::api::register_routes();
    routers::ui::register_routes();

    let app = quench_starter::http::discover_and_mount(base_path.clone());
    let app = routers::api::wrap_auth(app, jwt_config, &base_path);

    serve_app("gantry-service", app, container).await
}
