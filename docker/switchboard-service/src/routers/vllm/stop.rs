use crate::routers::models::mod_impl::can;
use crate::routers::vllm::engine::VllmEngine;
use actix_web::{HttpResponse, Responder, delete, http::header::ContentType, web};
use quench_auth::prelude::JwtConfig;
use std::sync::Arc;

#[delete("/instances/{id}")]
pub async fn stop_instance(
    http_req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    id: web::Path<String>,
    engine: web::Data<Arc<dyn VllmEngine>>,
) -> impl Responder {
    if !can(&http_req, &config, "stop") {
        return HttpResponse::Forbidden().finish();
    }

    match engine.stop_instance(id.into_inner()).await {
        Ok(_) => HttpResponse::Ok()
            .content_type(ContentType::html())
            .body(r#"<div id="confirm-stop-instance-modal" class="estimates-modal"></div>"#),
        Err(err) => {
            if err.to_lowercase().contains("not found") {
                tracing::warn!("vLLM instance to stop not found: {}", err);
                return HttpResponse::NotFound().body("api_error_instance_not_found");
            }
            tracing::error!("Failed to stop vLLM instance: {}", err);
            HttpResponse::InternalServerError().body("api_error_vllm_stop_failed")
        }
    }
}
