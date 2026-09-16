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
use switchboard_service::routers;
use switchboard_service::routers::gpu::{GpuBroadcaster, init_gpu_status_publisher};
use switchboard_service::routers::models::{init_model_store, start_sync_job, warm_model_cache};
use switchboard_service::routers::vllm::sse::VllmBroadcaster;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    tracing::info!("Switchboard service starting");

    let base_path = normalize_base_path(&envmnt::get_or("BASE_PATH", "/"));
    tracing::info!("Server initialized with BASE_PATH: {base_path}");

    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = JwtConfig::init().await;
    let sso_config = SsoConfig::init();
    let user_db = UserDb::init(db_wrapper.db.clone()).await;
    let session_db = SessionDb::from_env().await.expect("session store");

    init_model_store(db_wrapper.db.clone()).await;

    let (gpu_tx, _) = tokio::sync::broadcast::channel::<String>(100);
    let vllm_engine = routers::vllm::init_engine().await;
    routers::vllm::spawn_reaper(vllm_engine.clone());
    let (vllm_tx, _) = tokio::sync::broadcast::channel::<String>(100);

    let health_state = HealthState::live();
    let init_gpu_tx = gpu_tx.clone();
    let init_vllm_tx = vllm_tx.clone();
    let init_vllm_engine = vllm_engine.clone();
    let gatehouse_health_url = gatehouse_health_url();
    health_state.spawn_ready_when(async move {
        tokio::join!(
            wait_for_services("switchboard-service", vec![gatehouse_health_url.as_str()]),
            async {
                warm_model_cache().await;
                start_sync_job();
                init_gpu_status_publisher(init_gpu_tx);
                routers::vllm::init_vllm_status_publisher(init_vllm_tx, init_vllm_engine);
            }
        );
    });

    let container = ContainerBuilder::new()
        .provide(db_wrapper.db.clone())
        .provide(health_state)
        .provide(jwt_config.clone())
        .provide(sso_config)
        .provide_arc(user_db)
        .provide_arc(session_db)
        .provide(GpuBroadcaster(gpu_tx))
        .provide(VllmBroadcaster(vllm_tx))
        .provide(vllm_engine)
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    routers::gpu::register_routes();
    routers::models::register_routes();
    routers::vllm::register_routes();
    routers::ui::register_routes();

    let app = quench_starter::http::discover_and_mount(base_path.clone());
    let app = routers::models::wrap_auth(app, jwt_config.clone(), &base_path);
    let app = routers::gpu::wrap_auth(app, jwt_config.clone(), &base_path);
    let app = routers::vllm::wrap_auth(app, jwt_config, &base_path);

    serve_app("switchboard-service", app, container).await
}
