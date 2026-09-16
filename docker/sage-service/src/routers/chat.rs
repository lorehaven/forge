use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::{ChatMessage, VllmClient};
use crate::config::SageConfig;
use crate::observability::cost_tracking::CostTracker;
use crate::observability::metrics::MetricsCollector;
use bytes::Bytes;
use futures_util::StreamExt;
use quench_http::prelude::{HttpError, Inject, Json, Path, Response, get, http::StatusCode, post};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ChatRequest {
    pub instance_id: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct CapabilitiesResponse {
    pub profile: String,
    pub description: String,
    pub available_tools: Vec<String>,
    pub available_search_providers: Vec<String>,
}

#[derive(Serialize)]
pub struct MetricsResponse {
    pub profiles: Vec<crate::observability::metrics::ProfileMetrics>,
}

#[derive(Serialize)]
pub struct CostsResponse {
    pub users: Vec<crate::observability::cost_tracking::UserCosts>,
    pub profiles: Vec<crate::observability::cost_tracking::ProfileCosts>,
}

#[post("/api/v1/chat")]
pub async fn chat(
    Json(req): Json<ChatRequest>,
    Inject(switchboard): Inject<SwitchboardClient>,
    Inject(vllm): Inject<VllmClient>,
    Inject(config): Inject<SageConfig>,
) -> Response {
    let instances = match switchboard.get_vllm_instances().await {
        Ok(i) => i,
        Err(err) => {
            tracing::error!("Failed to get vLLM instances from Switchboard: {}", err);
            return Response::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_switchboard_unavailable",
            );
        }
    };

    let Some(instance) = instances.into_iter().find(|i| i.id == req.instance_id) else {
        return Response::text(StatusCode::NOT_FOUND, "api_error_instance_not_found");
    };

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: config.system_prompt.clone(),
            tool_calls: None,
            images: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: req.message.clone(),
            tool_calls: None,
            images: None,
        },
    ];

    let stream = match vllm
        .chat_stream(
            &instance.host,
            instance.port,
            &instance.model,
            messages,
            instance.max_model_len,
        )
        .await
    {
        Ok(s) => s,
        Err(err) => {
            tracing::error!("Failed to create vLLM chat stream: {}", err);
            return Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_stream_failed");
        }
    };

    let sse_stream = stream.map(|res| match res {
        Ok(content) => {
            let data = serde_json::json!({ "content": content });
            Ok::<_, std::io::Error>(Bytes::from(format!("data: {}\n\n", data)))
        }
        Err(err) => {
            // Localization code plus the raw detail for diagnostics.
            let data = serde_json::json!({ "error": "api_error_stream_failed", "detail": err.to_string() });
            Ok::<_, std::io::Error>(Bytes::from(format!("data: {}\n\n", data)))
        }
    });

    Response::streaming(StatusCode::OK, sse_stream).header("content-type", "text/event-stream")
}

#[get("/api/v1/chat/capabilities")]
pub async fn capabilities(
    Inject(config): Inject<SageConfig>,
) -> Result<Json<CapabilitiesResponse>, HttpError> {
    let profile = &config.capability_profile;
    let mut tools = profile.enabled_tool_names();
    tools.sort();

    Ok(Json(CapabilitiesResponse {
        profile: profile.name.clone(),
        description: profile.description.clone(),
        available_tools: tools.into_iter().map(|s| s.to_string()).collect(),
        available_search_providers: config.available_search_providers.clone(),
    }))
}

#[get("/api/v1/chat/metrics")]
pub async fn get_metrics(
    Inject(metrics_collector): Inject<MetricsCollector>,
) -> Json<MetricsResponse> {
    let profiles = metrics_collector.get_all_profiles_metrics();
    Json(MetricsResponse { profiles })
}

#[get("/api/v1/chat/metrics/{profile}")]
pub async fn get_metrics_by_profile(
    Path(profile): Path<String>,
    Inject(metrics_collector): Inject<MetricsCollector>,
) -> Response {
    match metrics_collector.get_profile_metrics(&profile) {
        Some(metrics) => Response::json(StatusCode::OK, &metrics)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        None => Response::text(StatusCode::NOT_FOUND, "api_error_metrics_not_found"),
    }
}

#[get("/api/v1/chat/costs")]
pub async fn get_costs(Inject(cost_tracker): Inject<CostTracker>) -> Json<CostsResponse> {
    let users = cost_tracker.get_all_user_costs();
    let profiles = cost_tracker.get_all_profile_costs();
    Json(CostsResponse { users, profiles })
}

#[get("/api/v1/chat/costs/user/{user_id}")]
pub async fn get_user_costs(
    Path(user_id): Path<String>,
    Inject(cost_tracker): Inject<CostTracker>,
) -> Response {
    match cost_tracker.get_user_costs(&user_id) {
        Some(costs) => Response::json(StatusCode::OK, &costs)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        None => Response::text(StatusCode::NOT_FOUND, "api_error_costs_not_found"),
    }
}

#[get("/api/v1/chat/context-status/{profile}")]
pub async fn get_context_status(
    Path(profile): Path<String>,
) -> Json<crate::domain::context::ContextStatus> {
    Json(crate::domain::context::ContextStatus::new(&profile, 0))
}

pub fn register_routes() {
    let _ = chat as fn(_, _, _, _) -> _;
    let _ = capabilities as fn(_) -> _;
    let _ = get_metrics as fn(_) -> _;
    let _ = get_metrics_by_profile as fn(_, _) -> _;
    let _ = get_costs as fn(_) -> _;
    let _ = get_user_costs as fn(_, _) -> _;
    let _ = get_context_status as fn(_) -> _;
}
