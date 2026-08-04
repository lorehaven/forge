use actix_web::web;
use switchboard_service::routers;
use switchboard_service::routers::gpu::init_gpu_status_publisher;
use switchboard_service::routers::models::{init_model_store, start_sync_job, warm_model_cache};
use switchboard_service::{base_path_scope, root_scope};

use quench_auth::actix::domain::sso_client::SsoConfig;
use quench_auth::prelude::*;
use quench_starter::prelude::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    tracing::info!("Switchboard service starting");

    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = web::Data::new(JwtConfig::init().await);
    let sso_config = web::Data::new(SsoConfig::init());
    let user_db = UserDb::init(db_wrapper.db.clone()).await;
    let session_db = SessionDb::from_env().await.expect("session store");

    init_model_store(db_wrapper.db.clone()).await;

    let (gpu_tx, _) = tokio::sync::broadcast::channel::<String>(100);
    let vllm_engine = routers::vllm::init_engine().await;
    routers::vllm::spawn_reaper(vllm_engine.clone());
    let (vllm_tx, _) = tokio::sync::broadcast::channel::<String>(100);

    let init_gpu_tx = gpu_tx.clone();
    let init_vllm_tx = vllm_tx.clone();
    let init_vllm_engine = vllm_engine.clone();
    let gatehouse_health_url = gatehouse_health_url();
    let init = async move {
        tokio::join!(
            wait_for_services("switchboard-service", vec![gatehouse_health_url.as_str()]),
            async {
                warm_model_cache().await;
                start_sync_job();
                init_gpu_status_publisher(init_gpu_tx);
                routers::vllm::init_vllm_status_publisher(init_vllm_tx, init_vllm_engine);
            }
        );
    };

    serve(
        root_scope,
        move || {
            base_path_scope(
                vllm_engine.clone(),
                gpu_tx.clone(),
                vllm_tx.clone(),
                jwt_config.clone(),
                sso_config.clone(),
                user_db.clone(),
                session_db.clone(),
            )
        },
        Some(db_wrapper),
        init,
    )
    .await
}
