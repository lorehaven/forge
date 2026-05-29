use crate::routers::models::{init_model_store, warm_model_cache};
use quench_srv::actix::domain::db::DbWrapper;
use quench_srv::prelude::{HttpServiceFactory, serve};

use crate::routers::vllm::engine::VllmEngine;
use std::sync::Arc;

pub mod routers;

pub fn root_scope() -> impl HttpServiceFactory {
    actix_web::web::scope("")
}

pub fn base_path_scope(vllm_engine: Arc<dyn VllmEngine>) -> impl HttpServiceFactory {
    actix_web::web::scope("")
        .service(routers::gpu::scope())
        .service(routers::models::scope())
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

    let vllm_engine = routers::vllm::init_engine().await;

    serve(
        root_scope,
        move || base_path_scope(vllm_engine.clone()),
        routers::openapi(),
        Some(db_wrapper),
    )
    .await
}
