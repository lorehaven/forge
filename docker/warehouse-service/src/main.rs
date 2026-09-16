use async_trait::async_trait;
use quench_auth::domain::auth::UserDb;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_auth::http::domain::sso_client::SsoConfig;
use quench_http::prelude::*;
use quench_starter::common::db::DbWrapper;
use quench_starter::common::health::HealthState;
use quench_starter::common::routes::normalize_base_path;
use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use quench_starter::http::serve_app;
use std::sync::Arc;
use warehouse_service::docker_token::DockerTokenConfig;
use warehouse_service::middleware::auth::WarehouseAuth;
use warehouse_service::middleware::limits::WarehouseLimits;
use warehouse_service::routers;

/// Sends `/v2/*` and `/token` to the raw docker router (fixed at the server root by protocol);
/// everything else to `base_path_app`, which handles `BASE_PATH` itself.
struct RootOrBasePath {
    docker: Arc<dyn Endpoint>,
    base_path_app: Arc<dyn Endpoint>,
}

#[async_trait]
impl Endpoint for RootOrBasePath {
    async fn call(&self, req: Request) -> Response {
        let path = req.uri().path();
        if path.starts_with("/v2/") || path == "/v2" || path == "/token" {
            self.docker.call(req).await
        } else {
            self.base_path_app.call(req).await
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    tracing::info!("Warehouse service starting");

    let base_path = normalize_base_path(&envmnt::get_or("BASE_PATH", "/"));
    tracing::info!("Server initialized with BASE_PATH: {base_path}");

    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = JwtConfig::init().await;
    let docker_token_config = DockerTokenConfig::init(
        jwt_config.service_name.clone(),
        jwt_config.realm.clone(),
        jwt_config.auth_enabled,
    );
    let sso_config = SsoConfig::init();
    let user_db = UserDb::init(db_wrapper.db.clone()).await;
    let session_db = SessionDb::from_env().await.expect("session store");
    let db = db_wrapper.db.clone();

    // Report configured storages at startup rather than via a caller's 404.
    routers::files::report_storages();

    // One-time move to the multi-platform APK layout; no-op once done.
    routers::artifacts::relocate_legacy_apk_storage();

    let health_state = HealthState::live();
    let gatehouse_health_url = gatehouse_health_url();
    health_state.spawn_ready_when(async move {
        wait_for_services("warehouse-service", vec![gatehouse_health_url.as_str()]).await;
    });

    let container = ContainerBuilder::new()
        .provide(db.clone())
        .provide(health_state)
        .provide(jwt_config.clone())
        .provide(docker_token_config.clone())
        .provide(sso_config)
        .provide_arc(user_db)
        .provide_arc(session_db)
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    routers::register_routes();

    let base_path_app = quench_starter::http::discover_and_mount(base_path.clone());
    let base_path_app = routers::files::wrap_auth(base_path_app, jwt_config.clone(), &base_path);
    let base_path_app = routers::artifacts::wrap_auth(base_path_app, jwt_config, &base_path);

    // WarehouseAuth self-restricts to /v2/* and passes /token through, so wrapping the whole tree is safe.
    let loader = quench_config::ConfigLoader::new("WAREHOUSE");
    let max_concurrent_uploads = loader.env_u64("MAX_CONCURRENT_UPLOADS", 32) as usize;
    let docker_router: Arc<dyn Endpoint> = Arc::new(discover_routes());
    let docker_router = wrap(docker_router, WarehouseLimits::new(max_concurrent_uploads));
    let docker_router = wrap(docker_router, WarehouseAuth::new(docker_token_config));

    let app: Arc<dyn Endpoint> = Arc::new(RootOrBasePath {
        docker: docker_router,
        base_path_app,
    });

    serve_app("warehouse-service", app, container).await
}
