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
        return Response::new(quench_http::http::StatusCode::FORBIDDEN);
    }

    match engine.stop_instance(id).await {
        Ok(()) => Response::html(
            quench_http::http::StatusCode::OK,
            r#"<div id="confirm-stop-instance-modal" class="estimates-modal"></div>"#,
        ),
        Err(err) => {
            if err.to_lowercase().contains("not found") {
                quench_log::warn!("vLLM instance to stop not found: {}", err);
                return Response::text(
                    quench_http::http::StatusCode::NOT_FOUND,
                    "api_error_instance_not_found",
                );
            }
            quench_log::error!("Failed to stop vLLM instance: {}", err);
            Response::text(
                quench_http::http::StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_vllm_stop_failed",
            )
        }
    }
}
