use crate::routers::gpu::{GpuBroadcaster, init_gpu_status_publisher};
use crate::routers::models::{init_model_store, start_sync_job, warm_model_cache};
use crate::routers::vllm::engine::VllmEngine;
use crate::routers::vllm::sse::VllmBroadcaster;

use quench_srv::prelude::*;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

pub mod routers;

pub fn root_scope() -> impl HttpServiceFactory {
    actix_web::web::scope("")
}

pub fn base_path_scope(
    vllm_engine: Arc<dyn VllmEngine>,
    gpu_tx: Sender<String>,
    vllm_tx: Sender<String>,
) -> impl HttpServiceFactory {
    actix_web::web::scope("")
        .app_data(actix_web::web::Data::new(GpuBroadcaster(gpu_tx)))
        .app_data(actix_web::web::Data::new(VllmBroadcaster(vllm_tx)))
        .service(routers::gpu::scope())
        .service(routers::models::scope(vllm_engine.clone()))
        .service(routers::vllm::scope(vllm_engine))
        .service(routers::ui::scope())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt().init();
    dotenvy::dotenv().ok();

    let db_wrapper = DbWrapper::init().await;
    init_model_store(db_wrapper.db.clone()).await;
    warm_model_cache().await;
    start_sync_job();

    let (gpu_tx, _) = tokio::sync::broadcast::channel::<String>(100);
    init_gpu_status_publisher(gpu_tx.clone());

    let vllm_engine = routers::vllm::init_engine().await;

    let (vllm_tx, _) = tokio::sync::broadcast::channel::<String>(100);
    routers::vllm::init_vllm_status_publisher(vllm_tx.clone(), vllm_engine.clone());

    serve(
        root_scope,
        move || base_path_scope(vllm_engine.clone(), gpu_tx.clone(), vllm_tx.clone()),
        Some(db_wrapper),
    )
    .await
}
