use crate::routers::vllm::engine::VllmEngine;
use actix_web::{HttpResponse, Responder, delete, web};
use std::sync::Arc;

#[delete("/instances/{id}")]
pub async fn stop_instance(
    id: web::Path<String>,
    engine: web::Data<Arc<dyn VllmEngine>>,
) -> impl Responder {
    match engine.stop_instance(id.into_inner()).await {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(err) => {
            tracing::error!("Failed to stop vLLM instance: {}", err);
            HttpResponse::InternalServerError().body(err)
        }
    }
}
