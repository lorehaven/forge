use super::mod_impl::is_admin;
use super::types::RunningModel;
use crate::routers::vllm::engine::VllmEngine;
use actix_web::{HttpResponse, Responder, get, web};
use quench_srv::prelude::JwtConfig;
use std::sync::Arc;

#[get("/running")]
pub async fn list_running_models(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    engine: web::Data<Arc<dyn VllmEngine>>,
) -> impl Responder {
    if !is_admin(&req, &config) {
        return HttpResponse::Forbidden().finish();
    }

    match engine.list_instances().await {
        Ok(instances) => {
            let running: Vec<RunningModel> = instances
                .into_iter()
                .map(|i| RunningModel {
                    id: i.id,
                    model: i.model,
                    endpoint: format!("http://{}:{}", i.host, i.port),
                    status: i.status,
                })
                .collect();
            HttpResponse::Ok().json(running)
        }
        Err(err) => {
            tracing::error!("Failed to list running models: {}", err);
            HttpResponse::InternalServerError().body(err)
        }
    }
}
