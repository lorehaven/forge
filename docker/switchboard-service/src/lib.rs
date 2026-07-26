use crate::routers::gpu::GpuBroadcaster;
use crate::routers::vllm::engine::VllmEngine;
use crate::routers::vllm::sse::VllmBroadcaster;
use actix_web::{dev::HttpServiceFactory, web};

use quench_auth::prelude::*;
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
    user_db: Arc<UserDb>,
    session_db: Arc<SessionDb>,
) -> impl HttpServiceFactory {
    web::scope("")
        .app_data(web::Data::new(GpuBroadcaster(gpu_tx)))
        .app_data(web::Data::new(VllmBroadcaster(vllm_tx)))
        .app_data(jwt_config.clone())
        .app_data(web::Data::new(user_db))
        .app_data(web::Data::new(session_db))
        .service(routers::ui::scope())
        .service(
            web::scope("")
                .wrap(quench_auth::actix::middleware::auth::Auth::new(
                    jwt_config.get_ref().clone(),
                ))
                .service(routers::gpu::scope())
                .service(routers::models::scope(vllm_engine.clone()))
                .service(routers::vllm::scope(vllm_engine)),
        )
}
