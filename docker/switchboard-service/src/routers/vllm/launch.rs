use super::types::LaunchRequest;
use crate::routers::models::mod_impl::can;
use crate::routers::vllm::engine::VllmEngine;
use actix_web::{HttpResponse, Responder, http::header::ContentType, post, web};
use quench_auth::prelude::JwtConfig;
use std::sync::Arc;

#[post("/instances")]
pub async fn launch_instance(
    http_req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    req: web::Json<LaunchRequest>,
    engine: web::Data<Arc<dyn VllmEngine>>,
) -> impl Responder {
    if !can(&http_req, &config, "launch") {
        return HttpResponse::Forbidden().finish();
    }

    let req = req.into_inner();
    if req.model.trim().is_empty() {
        return HttpResponse::BadRequest().body("api_error_model_name_empty");
    }

    match engine.launch_instance(req).await {
        Ok(instance) => HttpResponse::Accepted().json(instance),
        Err(err) => {
            tracing::error!("Failed to launch vLLM instance: {}", err);
            HttpResponse::InternalServerError().body("api_error_vllm_launch_failed")
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
    dtype: Option<String>,
    limit_mm_per_prompt: Option<String>,
    max_model_len: Option<String>,
    gpu_memory_utilization: Option<String>,
    enable_prefix_caching: Option<bool>,
    prefix_caching: Option<bool>,
    enable_tool_calling: Option<bool>,
    task: Option<String>,
}

#[post("/instances/form")]
pub async fn launch_instance_form(
    http_req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    form: web::Form<LaunchRequestForm>,
    engine: web::Data<Arc<dyn VllmEngine>>,
) -> impl Responder {
    if !can(&http_req, &config, "launch") {
        return HttpResponse::Forbidden().finish();
    }

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
        dtype: form.dtype.as_deref().and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }),
        limit_mm_per_prompt: form.limit_mm_per_prompt.as_deref().and_then(|value| {
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
        enable_tool_calling: form.enable_tool_calling.unwrap_or(false),
        task: form.task.as_deref().and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }),
    };

    match engine.launch_instance(req).await {
        Ok(_) => HttpResponse::Accepted()
            .content_type(ContentType::html())
            .body(r#"<div id="launch-modal" class="modal launch-modal"></div>"#),
        Err(err) => {
            tracing::error!("Failed to launch vLLM instance: {}", err);
            HttpResponse::InternalServerError().body("api_error_vllm_launch_failed")
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
