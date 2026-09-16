use super::types::{LaunchRequest, is_cpu_device};
use crate::routers::models::mod_impl::{OptionalClaims, can};
use crate::routers::vllm::engine::VllmEngine;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Form, Inject, Json, Response, post};
use std::sync::Arc;

#[post("/api/v1/vllm/instances")]
pub async fn launch_instance(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Json(req): Json<LaunchRequest>,
    Inject(engine): Inject<Arc<dyn VllmEngine>>,
) -> Response {
    if !can(claims.as_ref(), &config, "launch") {
        return Response::new(http::StatusCode::FORBIDDEN);
    }

    if req.model.trim().is_empty() {
        return Response::text(http::StatusCode::BAD_REQUEST, "api_error_model_name_empty");
    }

    match engine.launch_instance(req).await {
        Ok(instance) => Response::json(http::StatusCode::ACCEPTED, &instance)
            .unwrap_or_else(|_| Response::new(http::StatusCode::INTERNAL_SERVER_ERROR)),
        Err(err) => {
            tracing::error!("Failed to launch vLLM instance: {}", err);
            Response::text(
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_vllm_launch_failed",
            )
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
    device: Option<String>,
}

#[post("/api/v1/vllm/instances/form")]
pub async fn launch_instance_form(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Form(form): Form<LaunchRequestForm>,
    Inject(engine): Inject<Arc<dyn VllmEngine>>,
) -> Response {
    if !can(claims.as_ref(), &config, "launch") {
        return Response::new(http::StatusCode::FORBIDDEN);
    }

    let device = form.device.as_deref().and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    });
    // The GPU utilization field is disabled in the form for CPU launches, so
    // it arrives empty; make that explicit rather than falling back to 0.90.
    let gpu_memory_utilization = if is_cpu_device(device.as_deref()) {
        None
    } else {
        Some(parse_optional_f32(form.gpu_memory_utilization.as_deref()).unwrap_or(0.90))
    };

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
        gpu_memory_utilization,
        enable_prefix_caching: form
            .enable_prefix_caching
            .or(form.prefix_caching)
            .unwrap_or(false),
        enable_tool_calling: form.enable_tool_calling.unwrap_or(false),
        task: form.task.as_deref().and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }),
        device,
    };

    match engine.launch_instance(req).await {
        Ok(_) => Response::html(
            http::StatusCode::ACCEPTED,
            r#"<div id="launch-modal" class="modal launch-modal"></div>"#,
        ),
        Err(err) => {
            tracing::error!("Failed to launch vLLM instance: {}", err);
            Response::text(
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_vllm_launch_failed",
            )
        }
    }
}

pub fn parse_optional_u16(value: Option<&str>) -> Option<u16> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty())
            .then(|| value.parse::<u16>().ok())
            .flatten()
    })
}

pub fn parse_optional_u32(value: Option<&str>) -> Option<u32> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty())
            .then(|| value.parse::<u32>().ok())
            .flatten()
    })
}

pub fn parse_optional_f32(value: Option<&str>) -> Option<f32> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty())
            .then(|| value.parse::<f32>().ok())
            .flatten()
    })
}
