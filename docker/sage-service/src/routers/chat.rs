use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::{ChatMessage, VllmClient};
use crate::config::SageConfig;
use crate::observability::cost_tracking::CostTracker;
use crate::observability::metrics::MetricsCollector;
use quench_core::bytes::Bytes;
use quench_core::stream::StreamExt;
use quench_http::prelude::{HttpError, Inject, Json, Path, Response, get, http::StatusCode, post};
use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub instance_id: String,
    pub message: String,
    /// Earlier turns, oldest first (`user`/`assistant` only), for multi-turn use.
    #[serde(default)]
    pub history: Vec<HistoryMessage>,
    /// Search backend for this request (`duckduckgo`, `searxng`, `brave`, `serpapi`);
    /// the configured default when absent.
    #[serde(default)]
    pub search_provider: Option<String>,
    /// Fixed source passages. When present (even as an empty list, meaning "there are no
    /// sources"), planning, search and fetching are skipped and the answer is grounded on these,
    /// so models can be compared on identical evidence.
    #[serde(default)]
    pub evidence: Option<Vec<crate::grounding::EvidenceItem>>,
}

#[derive(Debug, Serialize)]
pub struct CapabilitiesResponse {
    pub profile: String,
    pub description: String,
    pub available_tools: Vec<String>,
    pub available_search_providers: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub profiles: Vec<crate::observability::metrics::ProfileMetrics>,
}

#[derive(Debug, Serialize)]
pub struct CostsResponse {
    pub users: Vec<crate::observability::cost_tracking::UserCosts>,
    pub profiles: Vec<crate::observability::cost_tracking::ProfileCosts>,
}

/// What a grounded answer is generated and checked against.
struct GroundedContext<'a> {
    cfg: &'a crate::grounding::GroundingConfig,
    switchboard: &'a SwitchboardClient,
    vllm: &'a VllmClient,
    instance: &'a crate::clients::switchboard::VllmInstance,
    today: &'a str,
    question: &'a str,
}

/// The user and assistant turns of the request's history, as chat messages.
fn history_messages(req: &ChatRequest) -> Vec<ChatMessage> {
    req.history
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            tool_calls: None,
            images: None,
        })
        .collect()
}

/// A grounded answer is checked as a whole, so it is returned in one piece: the answer, then the
/// sources it was built from.
async fn grounded_reply(
    ctx: &GroundedContext<'_>,
    g: &crate::grounding::Grounding,
    mut stream: std::pin::Pin<
        Box<dyn quench_core::stream::Stream<Item = quench_core::Result<String>> + Send + Sync>,
    >,
    retrieve_s: f64,
) -> Response {
    let mut answer = String::new();
    let generate_started = std::time::Instant::now();
    while let Some(res) = stream.next().await {
        match res {
            Ok(chunk) => answer.push_str(&chunk),
            Err(err) => {
                quench_log::error!("Grounded chat stream failed: {}", err);
                return Response::text(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "api_error_stream_failed",
                );
            }
        }
    }
    let generate_s = generate_started.elapsed().as_secs_f64();
    let answer = crate::grounding::strip_tool_calls(&answer);
    let verify_started = std::time::Instant::now();
    let answer = crate::grounding::finalize_answer(
        ctx.cfg,
        ctx.switchboard,
        ctx.vllm,
        ctx.instance,
        &answer,
        g.sources.len(),
        &g.evidence,
        ctx.today,
        ctx.question,
    )
    .await;
    let verify_s = verify_started.elapsed().as_secs_f64();
    let sources: Vec<_> = g
        .sources
        .iter()
        .map(|s| quench_serde::json!({ "index": s.index, "url": s.url, "domain": s.domain }))
        .collect();
    let events = vec![
        quench_serde::json!({ "content": answer }),
        quench_serde::json!({
            "grounded": true,
            "sources": sources,
            "search_failed": g.unavailable,
            "unsourced": g.unavailable,
            "timings": {
                "retrieve_s": (retrieve_s * 10.0).round() / 10.0,
                "generate_s": (generate_s * 10.0).round() / 10.0,
                "verify_s": (verify_s * 10.0).round() / 10.0,
            },
        }),
    ];
    let body = quench_core::stream::iter(
        events
            .into_iter()
            .map(|data| Ok::<_, std::io::Error>(Bytes::from(format!("data: {data}\n\n")))),
    );
    Response::streaming(StatusCode::OK, body).header("content-type", "text/event-stream")
}

/// The system prompt, the history, then the new user message.
fn build_messages(
    system_prompt: String,
    history: Vec<ChatMessage>,
    message: &str,
) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_prompt,
        tool_calls: None,
        images: None,
    }];
    messages.extend(history);
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: message.to_string(),
        tool_calls: None,
        images: None,
    });
    messages
}

/// The answer streamed as it is generated, one `data:` event per piece.
fn plain_reply(
    stream: std::pin::Pin<
        Box<dyn quench_core::stream::Stream<Item = quench_core::Result<String>> + Send + Sync>,
    >,
) -> Response {
    let sse_stream = stream.map(|res| match res {
        Ok(content) => {
            let data = quench_serde::json!({ "content": content });
            Ok::<_, std::io::Error>(Bytes::from(format!("data: {data}\n\n")))
        }
        Err(err) => {
            // Localization code plus the raw detail for diagnostics.
            let data = quench_serde::json!({ "error": "api_error_stream_failed", "detail": err.to_string() });
            Ok::<_, std::io::Error>(Bytes::from(format!("data: {data}\n\n")))
        }
    });

    Response::streaming(StatusCode::OK, sse_stream).header("content-type", "text/event-stream")
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
            quench_log::error!("Failed to get vLLM instances from Switchboard: {}", err);
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
    let today = quench_core::time::Time::now().format("%Y-%m-%d").clone();
    let mut system_prompt = config.system_prompt_with_date();
    let history = history_messages(&req);
    let mut grounding = None;
    let retrieve_started = std::time::Instant::now();
    if grounding_cfg.enabled
        && config
            .capability_profile
            .is_enabled(crate::tools::Tool::WebSearch)
    {
        grounding = if req.evidence.is_none() {
            crate::grounding::gather(
                &grounding_cfg,
                &switchboard,
                &vllm,
                &search_provider_registry,
                req.search_provider
                    .as_deref()
                    .unwrap_or(&config.default_search_provider),
                &instance,
                &history,
                &req.message,
                &today,
                None,
            )
            .await
        } else {
            Some(crate::grounding::from_evidence(
                req.evidence.as_deref().unwrap_or_default(),
                &today,
            ))
        };
        if let Some(g) = &grounding {
            system_prompt.push_str(&g.block);
        }
    }

    let messages = build_messages(system_prompt, history, &req.message);

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
            quench_log::error!("Failed to create vLLM chat stream: {}", err);
            return Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_stream_failed");
        }
    };

    let retrieve_s = retrieve_started.elapsed().as_secs_f64();

    // A grounded answer is checked as a whole, so it is returned in one piece: the answer,
    // then the sources it was built from.
    if let Some(g) = grounding {
        let context = GroundedContext {
            cfg: &grounding_cfg,
            switchboard: &switchboard,
            vllm: &vllm,
            instance: &instance,
            today: &today,
            question: &req.message,
        };
        return grounded_reply(&context, &g, stream, retrieve_s).await;
    }

    plain_reply(stream)
}

#[get("/api/v1/chat/capabilities")]
pub async fn capabilities(
    Inject(config): Inject<SageConfig>,
) -> Result<Json<CapabilitiesResponse>, HttpError> {
    let profile = &config.capability_profile;
    let mut tools = profile.enabled_tool_names();
    tools.sort_unstable();

    Ok(Json(CapabilitiesResponse {
        profile: profile.name.clone(),
        description: profile.description.clone(),
        available_tools: tools
            .into_iter()
            .map(std::string::ToString::to_string)
            .collect(),
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
    metrics_collector.get_profile_metrics(&profile).map_or_else(
        || Response::text(StatusCode::NOT_FOUND, "api_error_metrics_not_found"),
        |metrics| {
            Response::json(StatusCode::OK, &metrics)
                .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
        },
    )
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
    cost_tracker.get_user_costs(&user_id).map_or_else(
        || Response::text(StatusCode::NOT_FOUND, "api_error_costs_not_found"),
        |costs| {
            Response::json(StatusCode::OK, &costs)
                .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
        },
    )
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
