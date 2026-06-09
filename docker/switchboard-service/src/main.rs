use crate::routers::gpu::{GpuBroadcaster, init_gpu_status_publisher};
use crate::routers::models::{init_model_store, start_sync_job, warm_model_cache};
use crate::routers::vllm::engine::VllmEngine;
use crate::routers::vllm::sse::VllmBroadcaster;
use actix_web::{dev::HttpServiceFactory, web};

use quench_srv::prelude::*;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

pub mod routers;

pub fn root_scope() -> impl HttpServiceFactory {
    web::scope("")
}

pub fn base_path_scope(
    vllm_engine: Arc<dyn VllmEngine>,
    gpu_tx: Sender<String>,
    vllm_tx: Sender<String>,
    jwt_config: web::Data<JwtConfig>,
) -> impl HttpServiceFactory {
    web::scope("")
        .app_data(web::Data::new(GpuBroadcaster(gpu_tx)))
        .app_data(web::Data::new(VllmBroadcaster(vllm_tx)))
        .service(routers::ui::scope())
        .service(
            web::scope("")
                .wrap(quench_srv::actix::middleware::auth::Auth::new(
                    jwt_config.get_ref().clone(),
                ))
                .service(routers::gpu::scope())
                .service(routers::models::scope(vllm_engine.clone()))
                .service(routers::vllm::scope(vllm_engine)),
        )
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt().init();
    dotenvy::dotenv().ok();

    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = web::Data::new(JwtConfig::init());

    init_model_store(db_wrapper.db.clone()).await;

    let (gpu_tx, _) = tokio::sync::broadcast::channel::<String>(100);
    let vllm_engine = routers::vllm::init_engine().await;
    let (vllm_tx, _) = tokio::sync::broadcast::channel::<String>(100);

    let init_gpu_tx = gpu_tx.clone();
    let init_vllm_tx = vllm_tx.clone();
    let init_vllm_engine = vllm_engine.clone();
    let init = async move {
        warm_model_cache().await;
        start_sync_job();
        init_gpu_status_publisher(init_gpu_tx);
        routers::vllm::init_vllm_status_publisher(init_vllm_tx, init_vllm_engine);
    };

    serve(
        root_scope,
        move || {
            base_path_scope(
                vllm_engine.clone(),
                gpu_tx.clone(),
                vllm_tx.clone(),
                jwt_config.clone(),
            )
        },
        Some(db_wrapper),
        init,
    )
    .await
}
