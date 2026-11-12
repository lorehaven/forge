use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_auth::http::domain::sso_client::SsoConfig;
use quench_http::prelude::*;
use quench_starter::common::routes::normalize_base_path;
use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use quench_starter::http::serve_app;
use songbook_service::app::App;
use songbook_service::config::Config;
use songbook_service::deezer::Deezer;
use songbook_service::index::Index;
use songbook_service::jobs::Jobs;
use songbook_service::library::Library;
use songbook_service::playlists::Playlists;
use songbook_service::routers;
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // `songbook-service migrate-sqlite ...` moves a 0.1 library into Postgres and exits; it is not the server.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("migrate-sqlite") {
        return match songbook_service::migrate::command(&args[1..]).await {
            Ok(()) => Ok(()),
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        };
    }
    quench_starter::logging::init();
    quench_log::info!("Songbook starting");

    let base_path = normalize_base_path(&quench_core::env::get_or("BASE_PATH", "/"));
    quench_log::info!("Server initialized with BASE_PATH: {base_path}");

    let config = Config::from_env();
    assert!(
        !config.database_url.trim().is_empty(),
        "no database configured: set DATABASE_URL (or POSTGRES_URL); the tables are installed by Foundry"
    );
    quench_log::info!("schema {}, audio {}", config.schema, config.data.display());
    let pool = songbook_service::index::db::connect(&config.database_url, &config.schema)
        .await
        .unwrap_or_else(|e| panic!("database: {e}"));
    let index = Index::open(pool.clone())
        .await
        .unwrap_or_else(|e| panic!("index: {e}"));
    let playlists = Playlists::new(pool);
    let deezer = Deezer::live(config.cache());
    let app = App {
        config: Arc::new(config),
        index,
        jobs: Jobs::default(),
        deezer,
        library: Library::default(),
        playlists,
    };

    songbook_service::schedule::spawn(app.clone());

    let jwt_config = JwtConfig::init().await;
    let sso_config = SsoConfig::init();
    let session_db = SessionDb::from_env().await.expect("session store");

    let health_state = quench_starter::common::health::HealthState::live();
    let gatehouse_health_url = gatehouse_health_url();
    health_state.spawn_ready_when(async move {
        wait_for_services("songbook-service", vec![gatehouse_health_url.as_str()]).await;
    });

    let container = ContainerBuilder::new()
        .provide(app)
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

    serve_app("songbook-service", app, container).await
}
