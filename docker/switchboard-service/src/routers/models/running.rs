use super::mod_impl::{OptionalClaims, is_admin};
use super::types::RunningModel;
use crate::routers::vllm::engine::VllmEngine;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{HttpError, Inject, IntoResponse, Json, Response, get};
use std::sync::Arc;

#[get("/api/v1/models/running")]
pub async fn list_running_models(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(engine): Inject<Arc<dyn VllmEngine>>,
) -> Result<Response, HttpError> {
    if !is_admin(claims.as_ref(), &config) {
        return Ok(Response::new(http::StatusCode::FORBIDDEN));
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
            Ok(Json(running).into_response())
        }
        Err(err) => {
            tracing::error!("Failed to list running models: {}", err);
            Ok(Response::text(
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_vllm_list_failed",
            ))
        }
    }
}
