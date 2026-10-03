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
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct ChatRequest {
    pub instance_id: String,
    pub message: String,
    /// Earlier turns, oldest first (`user`/`assistant` only), for multi-turn use.
    #[serde(default)]
    pub history: Vec<HistoryMessage>,
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
    Inject(search_provider_registry): Inject<crate::tools::SearchProviderRegistry>,
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

    // Grounded mode retrieves in code (see `grounding`), same as the web UI.
    let grounding_cfg = crate::grounding::GroundingConfig::from_env();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut system_prompt = config.system_prompt_with_date();
    let history: Vec<ChatMessage> = req
        .history
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            tool_calls: None,
            images: None,
        })
        .collect();
    let mut grounding = None;
    if grounding_cfg.enabled
        && config
            .capability_profile
            .is_enabled(crate::tools::Tool::WebSearch)
    {
        grounding = crate::grounding::gather(
            &grounding_cfg,
            &switchboard,
            &vllm,
            &search_provider_registry,
            &config.default_search_provider,
            &instance,
            &history,
            &req.message,
            &today,
            None,
        )
        .await;
        if let Some(g) = &grounding {
            system_prompt.push_str(&g.block);
        }
    }

    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_prompt,
        tool_calls: None,
        images: None,
    }];
    messages.extend(history);
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: req.message.clone(),
        tool_calls: None,
        images: None,
    });

    // Leave room for the prompt: a max_tokens equal to the whole context is rejected.
    let max_tokens = instance.max_model_len.map(|m| m.min(2048));
    let stream = match vllm
        .chat_stream(
            &instance.host,
            instance.port,
            &instance.model,
            messages,
            max_tokens,
        )
        .await
    {
        Ok(s) => s,
        Err(err) => {
            tracing::error!("Failed to create vLLM chat stream: {}", err);
            return Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_stream_failed");
        }
    };

    // A grounded answer is checked as a whole, so it is returned in one piece: the answer,
    // then the sources it was built from.
    if let Some(g) = grounding {
        let mut stream = stream;
        let mut answer = String::new();
        while let Some(res) = stream.next().await {
            match res {
                Ok(chunk) => answer.push_str(&chunk),
                Err(err) => {
                    tracing::error!("Grounded chat stream failed: {}", err);
                    return Response::text(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "api_error_stream_failed",
                    );
                }
            }
        }
        let answer = crate::grounding::finalize_answer(
            &grounding_cfg,
            &switchboard,
            &vllm,
            &instance,
            &answer,
            g.sources.len(),
            &g.evidence,
            &today,
        )
        .await;
        let sources: Vec<_> = g
            .sources
            .iter()
            .map(|s| serde_json::json!({ "index": s.index, "url": s.url, "domain": s.domain }))
            .collect();
        let events = vec![
            serde_json::json!({ "content": answer }),
            serde_json::json!({ "grounded": true, "sources": sources }),
        ];
        let body = futures_util::stream::iter(
            events
                .into_iter()
                .map(|data| Ok::<_, std::io::Error>(Bytes::from(format!("data: {}\n\n", data)))),
        );
        return Response::streaming(StatusCode::OK, body)
            .header("content-type", "text/event-stream");
    }

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
    let _ = chat as fn(_, _, _, _, _) -> _;
    let _ = capabilities as fn(_) -> _;
    let _ = get_metrics as fn(_) -> _;
    let _ = get_metrics_by_profile as fn(_, _) -> _;
    let _ = get_costs as fn(_) -> _;
    let _ = get_user_costs as fn(_, _) -> _;
    let _ = get_context_status as fn(_) -> _;
}
