use crate::routers::models::mod_impl::{OptionalClaims, can};
use crate::routers::vllm::engine::VllmEngine;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Path, Response, delete};
use std::sync::Arc;

#[delete("/api/v1/vllm/instances/{id}")]
pub async fn stop_instance(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Path(id): Path<String>,
    Inject(engine): Inject<Arc<dyn VllmEngine>>,
) -> Response {
    if !can(claims.as_ref(), &config, "stop") {
        return Response::new(http::StatusCode::FORBIDDEN);
    }

    match engine.stop_instance(id).await {
        Ok(_) => Response::html(
            http::StatusCode::OK,
            r#"<div id="confirm-stop-instance-modal" class="estimates-modal"></div>"#,
        ),
        Err(err) => {
            if err.to_lowercase().contains("not found") {
                tracing::warn!("vLLM instance to stop not found: {}", err);
                return Response::text(http::StatusCode::NOT_FOUND, "api_error_instance_not_found");
            }
            tracing::error!("Failed to stop vLLM instance: {}", err);
            Response::text(
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_vllm_stop_failed",
            )
        }
    }
}
