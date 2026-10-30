use crate::routers::vllm::engine::VllmEngine;
use quench_http::prelude::{Inject, IntoResponse, Json, Response, get};
use std::sync::Arc;

#[get("/api/v1/vllm/list")]
pub async fn list_instances_canonical(Inject(engine): Inject<Arc<dyn VllmEngine>>) -> Response {
    list_instances_impl(engine).await
}

#[get("/api/v1/vllm/instances")]
pub async fn list_instances_alias(Inject(engine): Inject<Arc<dyn VllmEngine>>) -> Response {
    list_instances_impl(engine).await
}

async fn list_instances_impl(engine: Arc<Arc<dyn VllmEngine>>) -> Response {
    match engine.list_instances().await {
        Ok(list) => Json(list).into_response(),
        Err(err) => {
            quench_log::error!("Failed to list vLLM instances: {}", err);
            Response::text(
                quench_http::http::StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_vllm_list_failed",
            )
        }
    }
}
