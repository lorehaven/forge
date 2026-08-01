use crate::routers::gpu::GpuBroadcaster;
use crate::routers::vllm::engine::VllmEngine;
use crate::routers::vllm::sse::VllmBroadcaster;
use actix_web::{dev::HttpServiceFactory, web};

use quench_auth::actix::domain::sso_client::SsoConfig;
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
    sso_config: web::Data<SsoConfig>,
    user_db: Arc<UserDb>,
    session_db: Arc<SessionDb>,
) -> impl HttpServiceFactory {
    web::scope("")
        .app_data(web::Data::new(GpuBroadcaster(gpu_tx)))
        .app_data(web::Data::new(VllmBroadcaster(vllm_tx)))
        .app_data(jwt_config.clone())
        .app_data(sso_config)
        .app_data(web::Data::new(user_db))
        .app_data(web::Data::new(session_db))
        .service(routers::ui::scope())
        // Each API scope wraps its own `Auth` and `RequireWrite` now, rather
        // than sharing one pair of wraps the way this used to be structured:
        // `models::scope` needs `handle_list` to skip `RequireWrite` while
        // everything else in it does not, which only works if that scope
        // controls its own wrapping - see its module docs. `gpu` and `vllm`
        // have no such exception, but wrap themselves the same way for
        // consistency, and because `.wrap()` has to happen inside the function
        // that returns `impl HttpServiceFactory` - the opaque return type has
        // no `.wrap()` of its own for a caller to chain.
        .service(routers::gpu::scope(jwt_config.get_ref().clone()))
        .service(routers::vllm::scope(
            vllm_engine.clone(),
            jwt_config.get_ref().clone(),
        ))
        .service(routers::models::scope(
            vllm_engine,
            jwt_config.get_ref().clone(),
        ))
}
