use super::types::LaunchRequest;
use crate::routers::vllm::engine::VllmEngine;
use actix_web::{HttpResponse, Responder, post, web};
use std::sync::Arc;

#[post("/instances")]
pub async fn launch_instance(
    req: web::Json<LaunchRequest>,
    engine: web::Data<Arc<dyn VllmEngine>>,
) -> impl Responder {
    match engine.launch_instance(req.into_inner()).await {
        Ok(instance) => HttpResponse::Accepted().json(instance),
        Err(err) => {
            tracing::error!("Failed to launch vLLM instance: {}", err);
            HttpResponse::InternalServerError().body(err)
        }
    }
}
