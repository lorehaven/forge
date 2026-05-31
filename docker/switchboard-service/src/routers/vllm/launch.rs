use super::types::LaunchRequest;
use crate::routers::vllm::engine::VllmEngine;
use actix_web::{HttpResponse, Responder, http::header::ContentType, post, web};
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

#[derive(serde::Deserialize)]
pub struct LaunchRequestForm {
    model: String,
    host: Option<String>,
    port: Option<String>,
    namespace: Option<String>,
    quantization: Option<String>,
    max_model_len: Option<String>,
    gpu_memory_utilization: Option<String>,
    enable_prefix_caching: Option<bool>,
    prefix_caching: Option<bool>,
}

#[post("/instances/form")]
pub async fn launch_instance_form(
    form: web::Form<LaunchRequestForm>,
    engine: web::Data<Arc<dyn VllmEngine>>,
) -> impl Responder {
    let req = LaunchRequest {
        model: form.model.clone(),
        host: form.host.clone().unwrap_or_else(|| "0.0.0.0".to_string()),
        port: parse_optional_u16(form.port.as_deref()).unwrap_or(8000),
        namespace: form.namespace.as_deref().and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }),
        quantization: form.quantization.as_deref().and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }),
        max_model_len: parse_optional_u32(form.max_model_len.as_deref()),
        gpu_memory_utilization: Some(
            parse_optional_f32(form.gpu_memory_utilization.as_deref()).unwrap_or(0.90),
        ),
        enable_prefix_caching: form
            .enable_prefix_caching
            .or(form.prefix_caching)
            .unwrap_or(false),
    };

    match engine.launch_instance(req).await {
        Ok(_) => HttpResponse::Accepted()
            .content_type(ContentType::html())
            .body(r#"<div id="launch-modal" class="modal launch-modal"></div>"#),
        Err(err) => {
            tracing::error!("Failed to launch vLLM instance: {}", err);
            HttpResponse::InternalServerError().body(err)
        }
    }
}

fn parse_optional_u16(value: Option<&str>) -> Option<u16> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty())
            .then(|| value.parse::<u16>().ok())
            .flatten()
    })
}

fn parse_optional_u32(value: Option<&str>) -> Option<u32> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty())
            .then(|| value.parse::<u32>().ok())
            .flatten()
    })
}

fn parse_optional_f32(value: Option<&str>) -> Option<f32> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty())
            .then(|| value.parse::<f32>().ok())
            .flatten()
    })
}
