use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::{ChatMessage, VllmClient};
use crate::config::SageConfig;
use crate::routers::ui::common::RequiredClaims;
use crate::routers::ui::common::format::format_message;
use crate::tools::ToolExecutor;
use bytes::Bytes;
use dashmap::DashMap;
use futures_util::StreamExt;
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Path, Query, Response, get, http::StatusCode, post};
use quench_starter::common::routes::with_base_path;
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

pub struct ChatState {
    pub pending_messages: DashMap<String, ChatRequest>,
}

#[derive(Deserialize, Clone)]
pub struct ChatRequest {
    pub instance_id: String,
    pub message: String,
    pub conversation_id: String,
    pub project_id: Option<String>,
    pub search_provider: Option<String>,
    pub parent_id: Option<String>,
    pub capability_profile: Option<String>,
    #[serde(default)]
    pub tool_confirmations: Vec<String>,
    #[serde(default)]
    pub skip_user_message: bool,
    /// Comma-separated staged file ids - a single string because
    /// `serde_urlencoded` can't deserialize repeated keys into a `Vec`.
    #[serde(default)]
    pub file_ids: String,
}

impl ChatRequest {
    /// The staged file ids as a list, empty entries removed.
    pub fn file_id_list(&self) -> Vec<String> {
        self.file_ids
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }
}

#[post("/ui/chat/send")]
pub async fn send_message(
    claims: RequiredClaims,
    Form(form): Form<ChatRequest>,
    Inject(state): Inject<ChatState>,
    Inject(db): Inject<Db>,
) -> Response {
    let username = match claims.or_401() {
        Ok(claims) => claims.sub,
        Err(response) => return response,
    };

    let message_id = Uuid::new_v4().to_string();
    let mut chat_req = form;
    chat_req.message = chat_req.message.trim().to_string();

    state
        .pending_messages
        .insert(message_id.clone(), chat_req.clone());

    let mut stream_url = with_base_path(&format!("/ui/chat/stream/{}", message_id));
    if let Some(ref pid) = chat_req.project_id {
        stream_url = format!("{}?project_id={}", stream_url, pid);
    }

    let user_preview: String = chat_req.message.chars().take(30).collect();
    let user_preview = if chat_req.message.chars().count() > 30 {
        format!("{}...", user_preview)
    } else {
        user_preview
    };

    if chat_req.skip_user_message {
        // Regeneration skips re-showing the user message.
        let ai_msg = div()
            .class("chat-message message-ai")
            .attr("id", format!("ai-{}", message_id))
            .attr("hx-ext", "sse")
            .attr("sse-connect", stream_url)
            .attr("sse-swap", "message")
            .child(
                div().class("message-inner").child(
                    div()
                        .class("message-content")
                        .attr("data-i18n", "ui_chat_regenerating")
                        .text("Sage is regenerating..."),
                ),
            );

        return Response::html(StatusCode::OK, ai_msg.render());
    }

    let edit_btn = button()
        .class("branch-btn edit-btn")
        .attr(
            "hx-get",
            with_base_path(&format!("/ui/chat/edit-form/{}", message_id)),
        ) // Use pending ID, will be transitioned
        .attr("hx-target", format!("#user-{}", message_id))
        .attr("hx-swap", "innerHTML")
        .child(i().class("fas fa-edit"))
        .child(span().attr("data-i18n", "ui_chat_edit").text(" Edit"));

    let staged_ids = chat_req.file_id_list();
    let attachments_opt = if staged_ids.is_empty() {
        None
    } else {
        let files =
            crate::routers::ui::pages::files::load_owned_files(&db, &staged_ids, &username).await;
        crate::routers::ui::pages::files::render_attachments_row(&files)
    };

    let user_msg = div()
        .class("chat-message message-user")
        .attr("id", format!("user-{}", message_id))
        .child(
            div()
                .class("message-inner")
                .raw()
                .text(format_message(&chat_req.message))
                .child_opt(attachments_opt)
                .child(div().class("branch-controls").child(edit_btn)),
        );

    let ai_msg = div()
        .class("chat-message message-ai")
        .attr("id", format!("ai-{}", message_id))
        .attr("hx-ext", "sse")
        .attr("sse-connect", stream_url)
        .attr("sse-swap", "message")
        .child(
            div().class("message-inner").child(
                div()
                    .class("message-content")
                    .attr("data-i18n", "ui_chat_thinking")
                    .text("Sage is thinking..."),
            ),
        );

    let user_dot = div().attr("hx-swap-oob", "beforeend:.chat-navigation").child(
        div()
            .class("nav-dot")
            .attr("data-msg-id", format!("user-{}", message_id))
            .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
            .child(div().class("nav-tooltip").text(user_preview)),
    );

    let ai_dot = div().attr("hx-swap-oob", "beforeend:.chat-navigation").child(
        div()
            .class("nav-dot")
            .attr("id", format!("dot-ai-{}", message_id))
            .attr("data-msg-id", format!("ai-{}", message_id))
            .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
            .child(div().class("nav-tooltip").attr("id", format!("tooltip-ai-{}", message_id)).attr("data-i18n", "ui_chat_thinking").text("Sage is thinking...")),
    );

    Response::html(
        StatusCode::OK,
        format!(
            "{}{}{}{}",
            user_msg.render(),
            ai_msg.render(),
            user_dot.render(),
            ai_dot.render()
        ),
    )
}

pub fn encode_sse(event: &str, data: &str) -> Bytes {
    let mut sse = format!("event: {}\n", event);
    for line in data.split('\n') {
        sse.push_str("data: ");
        sse.push_str(line);
        sse.push('\n');
    }
    sse.push('\n');
    Bytes::from(sse)
}

/// Embed tool results into the response by replacing tool call markers
pub fn embed_tool_results_into_response(
    response: &str,
    tool_results_with_markers: Vec<(String, String)>, // (marker_text, result_html)
) -> String {
    let mut result = response.to_string();

    tracing::info!(
        "[EMBED] Starting embedding: {} markers, response_len={}",
        tool_results_with_markers.len(),
        response.len()
    );

    for (marker, html) in tool_results_with_markers {
        let marker_preview = if marker.len() > 100 {
            format!("{}...", &marker[..100])
        } else {
            marker.clone()
        };

        let found = response.contains(&marker);
        tracing::info!(
            "[EMBED] Looking for marker: found={}, marker_preview: {}",
            found,
            marker_preview
        );

        let before_len = result.len();
        result = result.replace(&marker, &format!("\n\n{}\n\n", html));
        let after_len = result.len();

        tracing::info!(
            "[EMBED] After replace: content_len_change={}",
            after_len as i32 - before_len as i32
        );
    }

    tracing::info!(
        "[EMBED] Embedding complete: final_response_len={}",
        result.len()
    );
    result
}

#[get("/ui/chat/stream/{id}")]
#[allow(clippy::too_many_arguments)]
pub async fn stream_message(
    claims: RequiredClaims,
    Path(id): Path<String>,
    Inject(state): Inject<ChatState>,
    Inject(switchboard): Inject<SwitchboardClient>,
    Inject(vllm): Inject<VllmClient>,
    Inject(config): Inject<SageConfig>,
    Inject(db): Inject<Db>,
    Inject(search_provider_registry): Inject<crate::tools::SearchProviderRegistry>,
    Inject(metrics_collector): Inject<crate::observability::metrics::MetricsCollector>,
    Inject(rate_limiter): Inject<tokio::sync::Mutex<crate::runtime::rate_limiter::RateLimiter>>,
    Inject(cost_tracker): Inject<crate::observability::cost_tracking::CostTracker>,
) -> Response {
    let username = match claims.or_401() {
        Ok(claims) => claims.sub,
        Err(response) => return response,
    };

    let message_id = id;

    let req = match state.pending_messages.get(&message_id) {
        Some(r) => r.clone(),
        None => return Response::new(StatusCode::NOT_FOUND),
    };

    tracing::info!(
        "Chat request: search_provider={:?}, instance_id={}",
        req.search_provider,
        req.instance_id
    );

    let active_profile = if let Some(requested_profile) = &req.capability_profile {
        match crate::tools::capabilities::get_profile(requested_profile) {
            Some(profile) => {
                tracing::info!("Using requested capability profile: {}", requested_profile);
                profile
            }
            None => {
                tracing::warn!(
                    "Requested profile '{}' not found, using default '{}'",
                    requested_profile,
                    config.capability_profile.name
                );
                config.capability_profile.clone()
            }
        }
    } else {
        config.capability_profile.clone()
    };

    let mut request_tool_registry = crate::tools::ToolRegistry::with_context(
        active_profile.clone(),
        Some(username.clone()),
        Some(req.conversation_id.clone()),
    );

    // Mirrors the global registry's own initialization.
    request_tool_registry.register(
        "web_search".to_string(),
        Box::new(crate::tools::web_search::WebSearchExecutor::new(
            search_provider_registry.clone(),
        )),
    );

    request_tool_registry.register(
        "calculator".to_string(),
        Box::new(crate::tools::calculator::CalculatorExecutor),
    );

    request_tool_registry.register(
        "web_fetch".to_string(),
        Box::new(crate::tools::web_fetch::WebFetchExecutor::new()),
    );

    request_tool_registry.register(
        "file_ops".to_string(),
        Box::new(crate::tools::file_ops::FileOpsExecutor::from_env()),
    );

    request_tool_registry.register(
        "file_search".to_string(),
        Box::new(crate::tools::file_search::FileSearchExecutor::new(
            (*db).clone(),
            (*switchboard).clone(),
            (*vllm).clone(),
            Some(req.conversation_id.clone()),
            req.project_id.clone(),
        )),
    );

    request_tool_registry.register(
        "file_list".to_string(),
        Box::new(crate::tools::file_list::FileListExecutor::new(
            (*db).clone(),
            Some(req.conversation_id.clone()),
            req.project_id.clone(),
        )),
    );

    request_tool_registry.register(
        "command".to_string(),
        Box::new(crate::tools::command::CommandExecutor::new()),
    );

    request_tool_registry.register(
        "code_executor".to_string(),
        Box::new(crate::tools::code_executor::CodeExecutor),
    );

    if !req.tool_confirmations.is_empty() {
        let confirmations: Vec<&str> = req.tool_confirmations.iter().map(|s| s.as_str()).collect();
        request_tool_registry.add_confirmations(&confirmations);
    }

    request_tool_registry.set_metrics_collector(metrics_collector.clone());
    request_tool_registry.set_rate_limiter(rate_limiter.clone());
    request_tool_registry.set_cost_tracker(cost_tracker.clone());

    // Shadows the global tool_registry param with this request-specific one.
    let tool_registry = Arc::new(request_tool_registry);

    let instances = match switchboard.get_vllm_instances().await {
        Ok(i) => i,
        Err(err) => {
            tracing::error!("Failed to get vLLM instances: {}", err);
            return Response::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_switchboard_unavailable",
            );
        }
    };

    let Some(instance) = instances.into_iter().find(|i| i.id == req.instance_id) else {
        return Response::text(StatusCode::NOT_FOUND, "api_error_instance_not_found");
    };

    if !instance.is_chat_capable() {
        tracing::warn!(
            "Chat request routed to non-chat instance '{}' (task={:?}); embedding instances do not serve chat completions",
            instance.id,
            instance.task
        );
        return Response::text(StatusCode::BAD_REQUEST, "api_error_embedding_model_chat");
    }

    let max_model_len = instance.max_model_len.unwrap_or(2048) as usize;
    let reserved_for_generation = if max_model_len > 4096 {
        2048
    } else if max_model_len > 2048 {
        1024
    } else {
        512
    };

    let prompt_budget = max_model_len.saturating_sub(reserved_for_generation);

    fn estimate_tokens(msg: &ChatMessage) -> usize {
        let image_tokens = msg.images.as_ref().map_or(0, |imgs| {
            imgs.len() * crate::files::images::image_token_estimate()
        });
        msg.content.chars().count().div_ceil(3) + 4 + image_tokens
    }

    let mut system_message = ChatMessage {
        role: "system".to_string(),
        content: config.system_prompt.clone(),
        tool_calls: None,
        images: None,
    };

    // Pre-create so RAG/file_list/file_search can resolve the conversation on
    // the first message, which Phase 5 would otherwise persist too late.
    {
        use quench_db::prelude::Crud;
        let conv_repo = db.repository::<crate::domain::models::Conversation>();
        if matches!(conv_repo.read(&req.conversation_id).await, Ok(None)) {
            let conv = crate::domain::models::Conversation {
                id: req.conversation_id.clone(),
                // Blank until Phase 5 derives the title from the message.
                title: String::new(),
                active_message_id: None,
                owner: username.clone(),
                project_id: req.project_id.clone(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            if let Err(e) = conv_repo.create(&conv).await {
                tracing::error!(
                    "Failed to pre-create conversation {}: {}",
                    req.conversation_id,
                    e
                );
            }
        }
    }

    // Advertise uploaded files and inject relevant excerpts when available.
    let mut injected_rag_hits: Vec<crate::files::rag::ChunkHit> = Vec::new();
    if let Some((rag_augmentation, hits)) = crate::files::rag::augment_system_prompt(
        &db,
        &switchboard,
        &vllm,
        &req.conversation_id,
        &req.message,
    )
    .await
    {
        system_message.content.push_str(&rag_augmentation);
        injected_rag_hits = hits;
    }

    tracing::info!(
        "System prompt length: {} chars, contains AVAILABLE TOOLS: {}",
        system_message.content.len(),
        system_message.content.contains("AVAILABLE TOOLS")
    );

    let has_web_search = system_message.content.contains("web_search");
    let has_tools_section = system_message.content.contains("AVAILABLE TOOLS");

    if has_tools_section && has_web_search {
        tracing::info!("✓ System prompt includes web_search tool definition");
    } else if has_tools_section && !has_web_search {
        tracing::warn!(
            "✗ System prompt has AVAILABLE TOOLS section but missing web_search definition"
        );
    } else {
        tracing::warn!("✗ System prompt does NOT include AVAILABLE TOOLS section");
    }

    let mut current_user_message = ChatMessage {
        role: "user".to_string(),
        content: req.message.clone(),
        tool_calls: None,
        images: None,
    };

    // Attach staged image uploads so vision models can see them; non-image attachments flow through RAG instead.
    if !req.skip_user_message {
        let staged_images =
            crate::files::images::load_staged_images(&db, &req.file_id_list(), &username).await;
        if !staged_images.is_empty() {
            current_user_message.images = Some(staged_images);
        }
    }

    tracing::info!("User message: {}", current_user_message.content);

    let system_tokens = estimate_tokens(&system_message);
    let current_user_tokens = estimate_tokens(&current_user_message);

    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::domain::models::Conversation>();
    let mut active_message_id = None;
    let mut existing_title = None;
    let mut existing_project_id = None;
    if let Ok(Some(conv)) = repo.read(&req.conversation_id).await {
        active_message_id = conv.active_message_id;
        existing_title = Some(conv.title);
        existing_project_id = conv.project_id;
    }

    // Regeneration bases history on parent_id instead of the active tip.
    let history_base_id = if req.skip_user_message {
        req.parent_id.as_deref()
    } else {
        active_message_id.as_deref()
    };

    let mut history_messages = Vec::new();
    if let Some(amid) = history_base_id
        && let Ok(msgs) = get_conversation_messages(&db, Some(amid)).await
    {
        history_messages = msgs;
    }

    let mut selected_history = std::collections::VecDeque::new();
    let mut current_budget_used = system_tokens
        + if req.skip_user_message {
            0
        } else {
            current_user_tokens
        };

    for msg in history_messages.into_iter().rev() {
        let msg_tokens = estimate_tokens(&msg);
        if current_budget_used + msg_tokens <= prompt_budget {
            current_budget_used += msg_tokens;
            selected_history.push_front(msg);
        } else {
            break;
        }
    }

    let mut messages = vec![system_message];
    messages.extend(selected_history);
    if !req.skip_user_message {
        messages.push(current_user_message);
    }

    // Stay within the vLLM instance's per-prompt image limit, preferring the newest images.
    crate::files::images::cap_images(
        &mut messages,
        crate::files::images::max_images_per_request(),
    );

    let max_tokens = reserved_for_generation as u32;

    // OpenAI tool-call format: {"type": "function", "function": {name, description, parameters}}
    let tool_definitions = tool_registry.get_definitions();
    let tools_json: Option<Vec<serde_json::Value>> = if !tool_definitions.is_empty() {
        let mut openai_tools = Vec::new();
        for tool_def in tool_definitions {
            let openai_tool = serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool_def.name,
                    "description": tool_def.description,
                    "parameters": tool_def.parameters
                }
            });
            openai_tools.push(openai_tool);
        }
        tracing::info!(
            "[VLLM_REQUEST] Sending {} tools to vLLM in OpenAI format",
            openai_tools.len()
        );
        Some(openai_tools)
    } else {
        None
    };

    let stream = match vllm
        .chat_stream_with_tools(
            &instance.host,
            instance.port,
            &instance.model,
            messages,
            Some(max_tokens),
            tools_json,
        )
        .await
    {
        Ok(s) => s,
        Err(err) => {
            tracing::error!("Failed to start chat stream: {}", err);
            return Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_stream_failed");
        }
    };

    let mut full_content = String::new();
    let message_id_clone = message_id.clone();
    let db_clone = db.clone();
    let vllm_clone = vllm.clone();
    let username_clone = username.clone();
    let tool_registry_clone = tool_registry.clone();
    let search_provider_registry_clone = search_provider_registry.clone();
    let config_clone = config.clone();

    // Channel-backed, not `async_stream::stream!`: DB work inside makes an
    // inline generator `!Sync`, but `ReceiverStream` is `Sync` regardless.
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(16);

    tokio::spawn(async move {
        let username = username_clone;
        let tool_registry = tool_registry_clone;
        let search_provider_registry = search_provider_registry_clone;
        let config = config_clone;
        let db_clone = db_clone;
        let vllm = vllm_clone;
        let mut stream = stream;

        // PHASE 1: stream response chunks in real time.
        tracing::info!("[STREAM_REFACTOR] Phase 1: Streaming response");
        while let Some(res) = stream.next().await {
            match res {
                Ok(content) => {
                    full_content.push_str(&content);
                    let formatted = format_message(&full_content);
                    let wrapped = format!("<div class=\"message-inner\">{}</div>", formatted);
                    if tx.send(encode_sse("message", &wrapped)).await.is_err() {
                        return;
                    }
                }
                Err(err) => {
                    let html = div()
                        .class("message-inner")
                        .child(
                            div()
                                .class("message-content")
                                .child(span().attr("data-i18n", "ui_chat_error").text("Error"))
                                .child(span().text(format!(": {}", err))),
                        )
                        .render();
                    let _ = tx.send(encode_sse("message", &html)).await;
                    return;
                }
            }
        }

        tracing::info!(
            "[STREAM_REFACTOR] Phase 1 complete: {} chars collected",
            full_content.len()
        );

        // PHASE 2: parse tool calls and check for meta-questions.
        tracing::info!(
            "[STREAM_REFACTOR] Phase 2: Parsing tool calls from {} chars",
            full_content.len()
        );

        let has_toolcall_tags =
            full_content.contains("<toolcall>") || full_content.contains("<tool_call>");
        tracing::debug!(
            "[PARSER] Response contains tool call tags: {}",
            has_toolcall_tags
        );

        let mut tool_calls = crate::tools::parser::parse_tool_calls(&full_content);
        tracing::info!("[STREAM_REFACTOR] Found {} tool calls", tool_calls.len());

        if tool_calls.is_empty() && has_toolcall_tags {
            tracing::warn!(
                "[PARSER] Tool call tags found but failed to parse them. Response preview: {}",
                &full_content[..full_content.len().min(500)]
            );
        }

        // Meta-questions about tools shouldn't trigger the tools themselves.
        let user_question_lower = req.message.to_lowercase();
        let is_meta_question = user_question_lower.contains("what tools")
            || user_question_lower.contains("what capabilities")
            || user_question_lower.contains("how do i use")
            || user_question_lower.contains("how do you use")
            || user_question_lower.contains("available tools")
            || user_question_lower.contains("can you do");

        if is_meta_question && !tool_calls.is_empty() {
            tracing::warn!(
                "[STREAM_REFACTOR] Suppressing {} tool calls for meta-question",
                tool_calls.len()
            );
            tool_calls.clear();
        }

        // PHASE 3: execute all tools and collect results with markers.
        tracing::info!(
            "[STREAM_REFACTOR] Phase 3: Executing {} tools",
            tool_calls.len()
        );
        let search_provider = req
            .search_provider
            .as_deref()
            .unwrap_or(&config.default_search_provider);

        // Map of marker string → formatted HTML result
        let mut tool_results_with_markers: Vec<(String, String)> = Vec::new();

        // Matches every tag variant the parser supports, mismatched/unclosed included.
        let tool_result_re = regex::Regex::new(
            r"(?s)<(?:tool_call|toolcall)>\s*(\{.*?\})\s*</(?:tool_call|toolcall)>",
        )
        .ok();

        for tool_call in &tool_calls {
            let query = tool_call
                .arguments
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            tracing::info!("[STREAM_REFACTOR] Executing tool: {}", tool_call.name);

            let result = if tool_call.name == "web_search" {
                let executor = crate::tools::web_search::WebSearchExecutor::new(
                    search_provider_registry.clone(),
                )
                .with_default_provider(search_provider.to_string());
                executor.execute(tool_call).await
            } else {
                tool_registry.execute(tool_call).await
            };

            // Runs the raw tool output back through vLLM for a clean summary.
            let formatted_result = if !result.is_error {
                let parse_prompt = format!(
                    "Results for: {}\n\n{}\n\nProvide the information clearly and concisely. No headers or title needed—just the useful content.",
                    query, result.content
                );

                let parse_messages = vec![
                    ChatMessage {
                        role: "system".to_string(),
                        content: "You are a helpful assistant. Present information clearly and concisely. Do not add titles, headers, or summaries—just the essential content.".to_string(),
                        tool_calls: None,
                        images: None,
                    },
                    ChatMessage {
                        role: "user".to_string(),
                        content: parse_prompt,
                        tool_calls: None,
                        images: None,
                    },
                ];

                let mut parsed = String::new();
                if let Ok(mut parse_stream) = vllm
                    .chat_stream(
                        &instance.host,
                        instance.port,
                        &instance.model,
                        parse_messages,
                        Some(512),
                    )
                    .await
                {
                    while let Some(res) = parse_stream.next().await {
                        if let Ok(content) = res {
                            parsed.push_str(&content);
                        }
                    }
                }

                let mut result_clone = result.clone();
                result_clone.content = parsed;
                render_tool_result(tool_call, &result_clone)
            } else {
                render_tool_result(tool_call, &result)
            };

            if let Some(re) = &tool_result_re {
                for marker_match in re.find_iter(&full_content) {
                    let marker_text = marker_match.as_str();
                    // Match by tool name first, then by argument content.
                    let mut matches = marker_text.contains(&tool_call.name);

                    if !matches && query != "unknown" {
                        matches = marker_text.contains(query);
                    }

                    // Fallback: any argument value appearing in the marker counts too.
                    if !matches {
                        for (_, val) in tool_call
                            .arguments
                            .as_object()
                            .unwrap_or(&serde_json::Map::new())
                        {
                            if let Some(s) = val.as_str()
                                && marker_text.contains(s)
                            {
                                matches = true;
                                break;
                            }
                        }
                    }

                    if matches {
                        tool_results_with_markers
                            .push((marker_text.to_string(), formatted_result.clone()));
                        tracing::info!(
                            "[STREAM_REFACTOR] Registered result for marker: {}",
                            &marker_text[..marker_text.len().min(50)]
                        );
                        break;
                    }
                }
            }
        }

        tracing::info!(
            "[STREAM_REFACTOR] Phase 3 complete: {} tool results collected",
            tool_results_with_markers.len()
        );

        // PHASE 4: embed tool results into response.
        tracing::info!("[STREAM_REFACTOR] Phase 4: Embedding results into response");

        // Stripped for storage; the display copy embeds tool results instead.
        let _clean_content = crate::tools::parser::strip_tool_calls(&full_content);

        let has_tool_results = !tool_results_with_markers.is_empty();

        let response_for_display = if has_tool_results {
            embed_tool_results_into_response(&full_content, tool_results_with_markers)
        } else {
            full_content.clone()
        };

        tracing::info!("[STREAM_REFACTOR] Phase 4 complete: Response ready for database");

        // PHASE 5: Database operations
        use quench_db::prelude::Crud;
        let conv_repo = db_clone.repository::<crate::domain::models::Conversation>();
        let updated_at = chrono::Utc::now().to_rfc3339();
        // A blank title means the conversation was created lazily; derive one from the message.
        let title = match existing_title {
            Some(t) if !t.trim().is_empty() => t,
            _ => {
                if req.message.chars().count() > 30 {
                    format!("{}...", req.message.chars().take(30).collect::<String>())
                } else {
                    req.message.clone()
                }
            }
        };

        let mut conv = crate::domain::models::Conversation {
            id: req.conversation_id.clone(),
            title,
            active_message_id: active_message_id.clone(),
            owner: username.clone(),
            // Falls back to the stored link so a message without project_id can't detach it.
            project_id: req.project_id.clone().or(existing_project_id),
            updated_at,
        };

        let exists = conv_repo
            .read(&req.conversation_id)
            .await
            .map(|o| o.is_some())
            .unwrap_or(false);
        if exists {
            if let Err(err) = conv_repo.update(&conv).await {
                tracing::error!("Failed to update conversation: {}", err);
            }
        } else {
            if let Err(err) = conv_repo.create(&conv).await {
                tracing::error!("Failed to create conversation: {}", err);
            }
        }

        let ai_parent_id = if !req.skip_user_message {
            let msg_repo = db_clone.repository::<crate::domain::models::Message>();
            let user_msg_id = uuid::Uuid::new_v4().to_string();
            let user_msg = crate::domain::models::Message {
                id: user_msg_id.clone(),
                conversation_id: req.conversation_id.clone(),
                parent_id: active_message_id.clone(),
                role: "user".to_string(),
                content: req.message.trim().to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            if let Err(err) = msg_repo.create(&user_msg).await {
                tracing::error!("Failed to create user message: {}", err);
            }
            // Link any files staged in the composer to this user message.
            let staged_ids = req.file_id_list();
            if !staged_ids.is_empty()
                && let Err(err) = crate::routers::files::link_files_to_message(
                    &db_clone,
                    &staged_ids,
                    &user_msg_id,
                    &req.conversation_id,
                    &username,
                )
                .await
            {
                tracing::error!("Failed to link attachments to message: {}", err);
            }
            Some(user_msg_id)
        } else {
            req.parent_id.clone()
        };

        // Stores the display copy, so embedded tool results persist too.
        let msg_repo = db_clone.repository::<crate::domain::models::Message>();
        let ai_msg_id = uuid::Uuid::new_v4().to_string();
        let ai_msg = crate::domain::models::Message {
            id: ai_msg_id.clone(),
            conversation_id: req.conversation_id.clone(),
            parent_id: ai_parent_id.clone(),
            role: "assistant".to_string(),
            content: response_for_display.trim().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Err(err) = msg_repo.create(&ai_msg).await {
            tracing::error!("Failed to create AI message: {}", err);
        }

        // Record auto-injected RAG sources against this message for attribution.
        if !injected_rag_hits.is_empty()
            && let Err(err) = crate::files::rag::record_rag_contexts(
                &db_clone,
                &ai_msg_id,
                &injected_rag_hits,
                "auto",
            )
            .await
        {
            tracing::error!("Failed to record RAG sources: {}", err);
        }

        conv.active_message_id = Some(ai_msg_id.clone());
        conv.updated_at = chrono::Utc::now().to_rfc3339();
        if let Err(err) = conv_repo.update(&conv).await {
            tracing::error!("Failed to update conversation tip: {}", err);
        }

        // PHASE 6: prepare final response with tool results embedded.
        tracing::info!("[STREAM_REFACTOR] Phase 6: Finalizing response with tools");

        // format_message already handles tool-result blocks.
        let final_content_html = format_message(&response_for_display);

        let mut controls = div().class("branch-controls");
        if let Ok(siblings) =
            get_siblings(&db_clone, &req.conversation_id, ai_parent_id.as_deref()).await
        {
            let total_siblings = siblings.len();
            let sibling_index = siblings.iter().position(|s| s.id == ai_msg_id).unwrap_or(0);

            if total_siblings > 1 {
                let prev_index = if sibling_index == 0 {
                    total_siblings - 1
                } else {
                    sibling_index - 1
                };
                let next_index = if sibling_index == total_siblings - 1 {
                    0
                } else {
                    sibling_index + 1
                };
                let prev_sibling = &siblings[prev_index];
                let next_sibling = &siblings[next_index];

                let nav = div()
                    .class("branch-nav")
                    .child(
                        form()
                            .attr("hx-post", with_base_path("/ui/chat/conversations/switch"))
                            .attr("style", "display: inline;")
                            .child(
                                input()
                                    .attr("type", "hidden")
                                    .attr("name", "conversation_id")
                                    .attr("value", &req.conversation_id),
                            )
                            .child(
                                input()
                                    .attr("type", "hidden")
                                    .attr("name", "target_message_id")
                                    .attr("value", &prev_sibling.id),
                            )
                            .child(
                                button()
                                    .class("branch-btn")
                                    .attr("type", "submit")
                                    .child(i().class("fas fa-chevron-left")),
                            ),
                    )
                    .child(span().class("branch-info").text(format!(
                        "{}/{}",
                        sibling_index + 1,
                        total_siblings
                    )))
                    .child(
                        form()
                            .attr("hx-post", with_base_path("/ui/chat/conversations/switch"))
                            .attr("style", "display: inline;")
                            .child(
                                input()
                                    .attr("type", "hidden")
                                    .attr("name", "conversation_id")
                                    .attr("value", &req.conversation_id),
                            )
                            .child(
                                input()
                                    .attr("type", "hidden")
                                    .attr("name", "target_message_id")
                                    .attr("value", &next_sibling.id),
                            )
                            .child(
                                button()
                                    .class("branch-btn")
                                    .attr("type", "submit")
                                    .child(i().class("fas fa-chevron-right")),
                            ),
                    );
                controls = controls.child(nav);
            }
        }

        let regenerate_btn = button()
            .class("branch-btn regenerate-btn")
            .attr("hx-post", with_base_path("/ui/chat/regenerate"))
            .attr("hx-vals", format!(r#"{{"message_id": "{}"}}"#, ai_msg_id))
            .attr("hx-target", ".chat-history")
            .attr("hx-swap", "beforeend")
            .child(i().class("fas fa-sync-alt"))
            .child(
                span()
                    .attr("data-i18n", "ui_chat_regenerate")
                    .text(" Regenerate"),
            );

        controls = controls.child(regenerate_btn);

        // Sources block from the excerpts auto-injected into this answer.
        let sources_opt = {
            let sources: Vec<crate::files::rag::RagSource> = injected_rag_hits
                .iter()
                .map(|h| crate::files::rag::RagSource {
                    file_name: h.file_name.clone(),
                    chunk_index: Some(h.chunk_index),
                    detail: h.detail.clone(),
                    similarity: Some(h.similarity),
                })
                .collect();
            crate::routers::ui::common::format::render_sources(&sources)
        };

        let message_inner = div()
            .class("message-inner")
            .raw()
            .text(&final_content_html)
            .child_opt(sources_opt)
            .child(div().class("branch-controls").raw().text(controls.render()));

        let oob_transition = div()
            .class("chat-message message-ai")
            .attr("id", format!("ai-{}", ai_msg_id))
            .attr("hx-swap-oob", format!("outerHTML:#ai-{}", message_id_clone))
            .child(message_inner);

        // 2. Transition the nav dot/tooltip IDs for the AI message.
        let ai_preview_raw: String = full_content.trim().chars().take(30).collect();
        let ai_preview = if full_content.trim().chars().count() > 30 {
            format!("{}...", ai_preview_raw)
        } else {
            ai_preview_raw
        };

        let ai_nav_dot_transition = div()
            .attr("hx-swap-oob", format!("outerHTML:#dot-ai-{}", message_id_clone))
            .child(
                div()
                    .class("nav-dot")
                    .attr("id", format!("dot-ai-{}", ai_msg_id))
                    .attr("data-msg-id", format!("ai-{}", ai_msg_id))
                    .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
                    .child(
                        div()
                            .class("nav-tooltip")
                            .attr("id", format!("tooltip-ai-{}", ai_msg_id))
                            .text(ai_preview),
                    ),
            );

        // 3. Transition the user message block to its permanent ID, add Edit.
        let mut user_oob_transition = String::new();
        let mut user_nav_dot_transition = String::new();

        if let Some(ref uid) = ai_parent_id
            && !req.skip_user_message
        {
            let edit_btn = button()
                .class("branch-btn edit-btn")
                .attr(
                    "hx-get",
                    with_base_path(&format!("/ui/chat/edit-form/{}", uid)),
                )
                .attr("hx-target", format!("#user-{}", uid))
                .attr("hx-swap", "innerHTML")
                .child(i().class("fas fa-edit"))
                .child(span().attr("data-i18n", "ui_chat_edit").text(" Edit"));

            let user_controls = div().class("branch-controls").child(edit_btn);

            // Must keep the attachment chips, or this OOB swap wipes them mid-stream.
            let staged_ids = req.file_id_list();
            let user_attachments_opt = if staged_ids.is_empty() {
                None
            } else {
                let files = crate::routers::ui::pages::files::load_owned_files(
                    &db_clone,
                    &staged_ids,
                    &username,
                )
                .await;
                crate::routers::ui::pages::files::render_attachments_row(&files)
            };

            user_oob_transition = div()
                .class("chat-message message-user")
                .attr("id", format!("user-{}", uid))
                .attr(
                    "hx-swap-oob",
                    format!("outerHTML:#user-{}", message_id_clone),
                )
                .child(
                    div()
                        .class("message-inner")
                        .raw()
                        .text(format_message(&req.message))
                        .child_opt(user_attachments_opt)
                        .child(user_controls),
                )
                .render();

            user_nav_dot_transition = div()
                    .attr("hx-swap-oob", format!("outerHTML:[data-msg-id='user-{}']", message_id_clone))
                    .child(
                        div()
                            .class("nav-dot")
                            .attr("data-msg-id", format!("user-{}", uid))
                            .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
                            .child(
                                div()
                                    .class("nav-tooltip")
                                    .text(req.message.chars().take(30).collect::<String>())
                            ),
                    )
                    .render();
        }

        let mut final_payload = format!(
            "{}{}{}{}",
            oob_transition.render(),
            ai_nav_dot_transition.render(),
            user_oob_transition,
            user_nav_dot_transition
        );

        // Generate updated history list for OOB swap
        if let Ok(mut conversations) = conv_repo.list().await {
            conversations.retain(|c| c.owner == username);
            conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

            let mut history_list = div()
                .class("history-list")
                .attr("id", "history-list")
                .attr("hx-swap-oob", "true");

            // Projects Section
            let project_repo = db_clone.repository::<crate::domain::models::Project>();
            if let Ok(mut projects) = project_repo.list().await {
                projects.retain(|p| p.owner == username);
                projects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

                let has_projects = !projects.is_empty();
                let projects_open_class = if has_projects { "open" } else { "" };

                history_list = history_list.child(
                    div()
                        .class(format!("history-section-header collapsible {}", projects_open_class))
                        .attr("onclick", "this.classList.toggle('open'); const content = this.nextElementSibling; if(content) { content.classList.toggle('hidden'); }")
                        .child(
                            div()
                                .attr("style", "display: flex; align-items: center; gap: 0.5rem;")
                                .child(i().class("fas fa-chevron-right chevron"))
                                .child(
                                    span()
                                        .attr("data-i18n", "ui_sidebar_projects")
                                        .text("Projects"),
                                )
                        )
                        .child(
                            button()
                                .class("branch-btn")
                                .attr("onclick", "event.stopPropagation();")
                                .attr("hx-get", with_base_path("/ui/projects/new-modal"))
                                .attr("hx-target", "body")
                                .attr("hx-swap", "beforeend")
                                .child(i().class("fas fa-plus"))
                                .child(span().attr("data-i18n", "ui_sidebar_new").text("New")),
                        ),
                );

                let mut projects_content = div().class("history-section-content");
                if !has_projects {
                    projects_content = projects_content.class("hidden");
                }

                for project in &projects {
                    let is_active = Some(project.id.clone()) == req.project_id;
                    let item_class = if is_active {
                        "history-item active project-item"
                    } else {
                        "history-item project-item"
                    };
                    let link_class = if is_active {
                        "history-item-link active"
                    } else {
                        "history-item-link"
                    };
                    let icon_class = if is_active {
                        "fas fa-folder-open"
                    } else {
                        "fas fa-folder"
                    };

                    let item = div().class(item_class).child(
                        a().class(link_class)
                            .attr(
                                "href",
                                with_base_path(&format!("/ui/home?project_id={}", project.id)),
                            )
                            .child(i().class(icon_class).attr("style", "margin-right: 8px;"))
                            .child(span().text(&project.name)),
                    );
                    projects_content = projects_content.child(item);

                    // Or the OOB sidebar swap drops it until a full page reload.
                    if is_active {
                        let files = crate::routers::files::visible_files_for_project(
                            &db_clone,
                            &project.id,
                        )
                        .await
                        .unwrap_or_default();
                        projects_content = projects_content.child(
                            crate::routers::ui::pages::files::render_project_files_section(&files),
                        );
                    }

                    let project_convs: Vec<_> = conversations
                        .iter()
                        .filter(|c| c.project_id.as_deref() == Some(&project.id))
                        .collect();
                    for conv_item in project_convs {
                        let is_conv_active = conv_item.id == req.conversation_id;
                        let conv_item_class = if is_conv_active {
                            "history-item active project-conv-item"
                        } else {
                            "history-item project-conv-item"
                        };
                        let conv_link_class = if is_conv_active {
                            "history-item-link active"
                        } else {
                            "history-item-link"
                        };
                        let item_id = format!("history-item-{}", conv_item.id);
                        let conv_url = format!(
                            "/ui/home?conversation_id={}&project_id={}",
                            conv_item.id, project.id
                        );

                        let item = div().class(conv_item_class).attr("id", &item_id).child(
                            a().class(conv_link_class).attr("href", with_base_path(&conv_url)).text(&conv_item.title)
                        ).child(
                            div().class("menu-container").child(
                                button().class("menu-trigger-btn").child(i().class("fas fa-ellipsis-v"))
                            ).child(
                                div().class("dropdown-menu").child(
                                    button().class("dropdown-item delete-item")
                                    .attr("hx-get", with_base_path(&format!("/ui/chat/conversations/delete-modal/{}?active_id={}", conv_item.id, req.conversation_id)))
                                    .attr("hx-target", "#confirm-delete-modal")
                                    .attr("hx-swap", "outerHTML")
                                    .child(i().class("fas fa-trash"))
                                    .child(span().attr("data-i18n", "ui_common_delete").text("Delete"))
                                )
                            )
                        );
                        projects_content = projects_content.child(item);
                    }
                }
                history_list = history_list.child(projects_content);
            }

            // Conversations Section
            let conv_header_text = "History";
            history_list = history_list.child(
                div()
                    .class("history-section-header collapsible open")
                    .attr("style", "margin-top: 0.75rem;")
                    .attr("onclick", "this.classList.toggle('open'); const content = this.nextElementSibling; if(content) { content.classList.toggle('hidden'); }")
                    .child(
                        div()
                            .attr("style", "display: flex; align-items: center; gap: 0.5rem;")
                            .child(i().class("fas fa-chevron-right chevron"))
                            .child(
                                span()
                                    .attr("data-i18n", "ui_sidebar_history")
                                    .text(conv_header_text),
                            )
                    )
            );

            let mut global_content = div().class("history-section-content");

            let global_convs: Vec<_> = conversations
                .iter()
                .filter(|c| c.project_id.is_none())
                .collect();
            for conv_item in global_convs {
                let is_active = conv_item.id == req.conversation_id;
                let item_class = if is_active {
                    "history-item active"
                } else {
                    "history-item"
                };
                let link_class = if is_active {
                    "history-item-link active"
                } else {
                    "history-item-link"
                };
                let item_id = format!("history-item-{}", conv_item.id);
                let conv_url = format!("/ui/home?conversation_id={}", conv_item.id);

                let item = div()
                    .class(item_class)
                    .attr("id", &item_id)
                    .child(
                        a()
                            .class(link_class)
                            .attr("href", with_base_path(&conv_url))
                            .text(&conv_item.title)
                    )
                    .child(
                        div()
                            .class("menu-container")
                            .child(
                                button()
                                    .class("menu-trigger-btn")
                                    .child(i().class("fas fa-ellipsis-v"))
                            )
                            .child(
                                div()
                                    .class("dropdown-menu")
                                    .child(
                                        button()
                                            .class("dropdown-item delete-item")
                                            .attr("hx-get", with_base_path(&format!("/ui/chat/conversations/delete-modal/{}?active_id={}", conv_item.id, req.conversation_id)))
                                            .attr("hx-target", "#confirm-delete-modal")
                                            .attr("hx-swap", "outerHTML")
                                            .child(i().class("fas fa-trash"))
                                            .child(
                                                span()
                                                    .attr("data-i18n", "ui_common_delete")
                                                    .text("Delete"),
                                            )
                                    )
                            )
                    );
                global_content = global_content.child(item);
            }
            history_list = history_list.child(global_content);
            final_payload.push_str(&history_list.render());
        }

        tracing::info!(
            "[STREAM_REFACTOR] Phase 6 complete: Sending final response with {} tool results",
            if has_tool_results { "some" } else { "no" }
        );

        if !final_payload.is_empty() {
            let _ = tx.send(encode_sse("message", &final_payload)).await;
        }

        state.pending_messages.remove(&message_id_clone);
    });

    let sse_stream = ReceiverStream::new(rx).map(Ok::<_, std::io::Error>);
    Response::streaming(StatusCode::OK, sse_stream).header("content-type", "text/event-stream")
}

#[derive(serde::Deserialize)]
pub struct DeleteQuery {
    pub active_id: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct DeleteModalQuery {
    pub active_id: Option<String>,
}

/// `"empty"` is a sentinel `id`, not its own route - two routes here would
/// race on registration order for which one matches `/empty`.
pub async fn delete_modal_empty() -> Response {
    let empty = div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal");
    Response::html(StatusCode::OK, empty.render())
}

#[get("/ui/chat/conversations/delete-modal/{id}")]
pub async fn delete_modal(
    Path(conv_id): Path<String>,
    Query(query): Query<DeleteModalQuery>,
    Inject(db): Inject<Db>,
) -> Response {
    if conv_id == "empty" {
        return delete_modal_empty().await;
    }
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::domain::models::Conversation>();

    let title = match repo.read(&conv_id).await {
        Ok(Some(conv)) => Some(format!("\"{}\"", conv.title)),
        _ => None,
    };

    let active_id_val = query.active_id.clone().unwrap_or_default();

    let modal = div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal open")
        .child(
            button()
                .class("estimates-modal-backdrop")
                .attr("type", "button")
                .attr(
                    "hx-get",
                    with_base_path("/ui/chat/conversations/delete-modal/empty"),
                )
                .attr("hx-target", "#confirm-delete-modal")
                .attr("hx-swap", "outerHTML"),
        )
        .child(
            div()
                .class("estimates-modal-content small")
                .child(
                    div()
                        .class("estimates-modal-header")
                        .child(
                            div()
                                .class("estimates-modal-title")
                                .attr("data-i18n", "ui_modal_delete_title")
                                .text("Confirm Delete"),
                        )
                        .child(
                            button()
                                .class("estimates-modal-close")
                                .attr("type", "button")
                                .attr(
                                    "hx-get",
                                    with_base_path("/ui/chat/conversations/delete-modal/empty"),
                                )
                                .attr("hx-target", "#confirm-delete-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(i().class("fas fa-times")),
                        ),
                )
                .child(
                    div()
                        .class("estimates-modal-body")
                        .child(
                            p().attr("data-i18n", "ui_chat_delete_confirm_text")
                                .text("Are you sure you want to delete this conversation?"),
                        )
                        .child(match title {
                            Some(title) => div().class("model-to-delete-name").text(title),
                            None => div()
                                .class("model-to-delete-name")
                                .attr("data-i18n", "ui_chat_this_conversation")
                                .text("this conversation"),
                        })
                        .child(
                            form()
                                .class("confirm-actions")
                                .attr(
                                    "hx-post",
                                    with_base_path(&format!(
                                        "/ui/chat/conversations/delete/{}?active_id={}",
                                        conv_id, active_id_val
                                    )),
                                )
                                .attr("hx-target", "#confirm-delete-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(
                                    button()
                                        .class("button cancel")
                                        .attr("type", "button")
                                        .attr(
                                            "hx-get",
                                            with_base_path(
                                                "/ui/chat/conversations/delete-modal/empty",
                                            ),
                                        )
                                        .attr("hx-target", "#confirm-delete-modal")
                                        .attr("hx-swap", "outerHTML")
                                        .attr("data-i18n", "ui_common_cancel")
                                        .text("Cancel"),
                                )
                                .child(
                                    button()
                                        .class("button danger")
                                        .attr("type", "submit")
                                        .attr("data-i18n", "ui_common_delete")
                                        .text("Delete"),
                                ),
                        ),
                ),
        );

    Response::html(StatusCode::OK, modal.render())
}

#[post("/ui/chat/conversations/delete/{id}")]
pub async fn delete_conversation(
    Path(id_str): Path<String>,
    Query(query): Query<DeleteQuery>,
    Inject(db): Inject<Db>,
) -> Response {
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::domain::models::Conversation>();
    let _ = repo.delete(&id_str).await;

    if query.active_id.as_deref() == Some(&id_str) {
        return Response::new(StatusCode::OK).header("HX-Redirect", with_base_path("/ui/home"));
    }

    let close_modal = div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal")
        .render();
    let oob_delete = div()
        .attr("id", format!("history-item-{}", id_str))
        .attr("hx-swap-oob", "delete")
        .render();

    Response::html(StatusCode::OK, format!("{}{}", close_modal, oob_delete))
}

#[derive(serde::Deserialize)]
pub struct SwitchBranchRequest {
    pub conversation_id: String,
    pub target_message_id: String,
}

#[post("/ui/chat/conversations/switch")]
pub async fn switch_branch(
    Form(form): Form<SwitchBranchRequest>,
    Inject(db): Inject<Db>,
) -> Response {
    if let Err(err) =
        switch_active_message(&db, &form.conversation_id, &form.target_message_id).await
    {
        tracing::error!("Failed to switch active branch: {}", err);
        return Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal");
    }

    Response::new(StatusCode::OK).header(
        "HX-Redirect",
        with_base_path(&format!(
            "/ui/home?conversation_id={}",
            form.conversation_id
        )),
    )
}

pub async fn get_conversation_messages(
    db: &quench_db::prelude::Db,
    active_message_id: Option<&str>,
) -> Result<Vec<ChatMessage>, anyhow::Error> {
    let mut chat_messages = Vec::new();
    let Some(mut current_id) = active_message_id.map(|s| s.to_string()) else {
        return Ok(chat_messages);
    };

    match db {
        quench_db::prelude::Db::Postgres(pg_db) => {
            let schema = envmnt::get_or("DB_SCHEMA", "sage");
            let table = format!("{}.messages", schema);
            let query = format!(
                "WITH RECURSIVE thread AS (
                    SELECT id, parent_id, role, content, created_at, 0 as depth
                    FROM {}
                    WHERE id = $1
                    UNION ALL
                    SELECT m.id, m.parent_id, m.role, m.content, m.created_at, t.depth + 1
                    FROM {} m
                    INNER JOIN thread t ON t.parent_id = m.id
                )
                SELECT id, role, content FROM thread ORDER BY depth DESC",
                table, table
            );

            let rows =
                sqlx::query_as::<_, (String, String, String)>(sqlx::AssertSqlSafe(query.as_str()))
                    .bind(current_id)
                    .fetch_all(pg_db.pool())
                    .await?;

            // So vision models keep seeing them in follow-up turns.
            let message_ids: Vec<String> = rows.iter().map(|(id, _, _)| id.clone()).collect();
            let mut images_by_message =
                crate::files::images::load_images_for_messages(db, &message_ids).await;

            for (id, role, content) in rows {
                chat_messages.push(ChatMessage {
                    role,
                    content,
                    tool_calls: None,
                    images: images_by_message.remove(&id),
                });
            }
        }
        quench_db::prelude::Db::InMemory(_mem_db) => {
            use quench_db::prelude::Crud;
            let repo = db.repository::<crate::domain::models::Message>();
            let mut visited = std::collections::HashSet::new();
            let mut message_list = Vec::new();
            while !current_id.is_empty() && visited.insert(current_id.clone()) {
                if let Ok(Some(msg)) = repo.read(&current_id).await {
                    message_list.push(msg.clone());
                    if let Some(pid) = msg.parent_id {
                        current_id = pid;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            message_list.reverse();
            for msg in message_list {
                chat_messages.push(ChatMessage {
                    role: msg.role,
                    content: msg.content,
                    tool_calls: None,
                    images: None,
                });
            }
        }
    }

    Ok(chat_messages)
}

pub async fn get_conversation_message_nodes(
    db: &quench_db::prelude::Db,
    active_message_id: Option<&str>,
) -> Result<Vec<crate::domain::models::Message>, anyhow::Error> {
    let mut message_nodes = Vec::new();
    let Some(mut current_id) = active_message_id.map(|s| s.to_string()) else {
        return Ok(message_nodes);
    };

    match db {
        quench_db::prelude::Db::Postgres(pg_db) => {
            let schema = envmnt::get_or("DB_SCHEMA", "sage");
            let table = format!("{}.messages", schema);
            let query = format!(
                "WITH RECURSIVE thread AS (
                    SELECT id, conversation_id, parent_id, role, content, created_at, 0 as depth
                    FROM {}
                    WHERE id = $1
                    UNION ALL
                    SELECT m.id, m.conversation_id, m.parent_id, m.role, m.content, m.created_at, t.depth + 1
                    FROM {} m
                    INNER JOIN thread t ON t.parent_id = m.id
                )
                SELECT id, conversation_id, parent_id, role, content, created_at FROM thread ORDER BY depth DESC",
                table, table
            );

            let rows =
                sqlx::query_as::<_, (String, String, Option<String>, String, String, String)>(
                    sqlx::AssertSqlSafe(query.as_str()),
                )
                .bind(current_id)
                .fetch_all(pg_db.pool())
                .await?;

            for (id, conversation_id, parent_id, role, content, created_at) in rows {
                message_nodes.push(crate::domain::models::Message {
                    id,
                    conversation_id,
                    parent_id,
                    role,
                    content,
                    created_at,
                });
            }
        }
        quench_db::prelude::Db::InMemory(_mem_db) => {
            use quench_db::prelude::Crud;
            let repo = db.repository::<crate::domain::models::Message>();
            let mut visited = std::collections::HashSet::new();
            let mut list = Vec::new();
            while !current_id.is_empty() && visited.insert(current_id.clone()) {
                if let Ok(Some(msg)) = repo.read(&current_id).await {
                    list.push(msg.clone());
                    if let Some(pid) = msg.parent_id {
                        current_id = pid;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            list.reverse();
            message_nodes = list;
        }
    }

    Ok(message_nodes)
}

pub async fn get_siblings(
    db: &quench_db::prelude::Db,
    conversation_id: &str,
    parent_id: Option<&str>,
) -> Result<Vec<crate::domain::models::Message>, anyhow::Error> {
    match db {
        quench_db::prelude::Db::Postgres(pg_db) => {
            let schema = envmnt::get_or("DB_SCHEMA", "sage");
            let table = format!("{}.messages", schema);
            let query = if let Some(_pid) = parent_id {
                format!(
                    "SELECT id, conversation_id, parent_id, role, content, created_at
                     FROM {}
                     WHERE conversation_id = $1 AND parent_id = $2
                     ORDER BY created_at ASC",
                    table
                )
            } else {
                format!(
                    "SELECT id, conversation_id, parent_id, role, content, created_at
                     FROM {}
                     WHERE conversation_id = $1 AND parent_id IS NULL
                     ORDER BY created_at ASC",
                    table
                )
            };

            let mut q =
                sqlx::query_as::<_, (String, String, Option<String>, String, String, String)>(
                    sqlx::AssertSqlSafe(query.as_str()),
                )
                .bind(conversation_id);
            if let Some(pid) = parent_id {
                q = q.bind(pid);
            }
            let rows = q.fetch_all(pg_db.pool()).await?;
            let mut siblings = Vec::new();
            for (id, conversation_id, parent_id, role, content, created_at) in rows {
                siblings.push(crate::domain::models::Message {
                    id,
                    conversation_id,
                    parent_id,
                    role,
                    content,
                    created_at,
                });
            }
            Ok(siblings)
        }
        quench_db::prelude::Db::InMemory(_mem_db) => {
            use quench_db::prelude::Crud;
            let repo = db.repository::<crate::domain::models::Message>();
            let all = repo.list().await?;
            let mut siblings: Vec<_> = all
                .into_iter()
                .filter(|m| {
                    m.conversation_id == conversation_id && m.parent_id.as_deref() == parent_id
                })
                .collect();
            siblings.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            Ok(siblings)
        }
    }
}

pub async fn switch_active_message(
    db: &quench_db::prelude::Db,
    conversation_id: &str,
    target_message_id: &str,
) -> Result<(), anyhow::Error> {
    let mut current_id = target_message_id.to_string();

    match db {
        quench_db::prelude::Db::Postgres(pg_db) => {
            let schema = envmnt::get_or("DB_SCHEMA", "sage");
            let table = format!("{}.messages", schema);

            loop {
                let query = format!(
                    "SELECT id FROM {} WHERE conversation_id = $1 AND parent_id = $2 ORDER BY created_at DESC LIMIT 1",
                    table
                );
                let child_opt: Option<(String,)> =
                    sqlx::query_as(sqlx::AssertSqlSafe(query.as_str()))
                        .bind(conversation_id)
                        .bind(&current_id)
                        .fetch_optional(pg_db.pool())
                        .await?;

                if let Some((child_id,)) = child_opt {
                    current_id = child_id;
                } else {
                    break;
                }
            }
        }
        quench_db::prelude::Db::InMemory(_mem_db) => {
            use quench_db::prelude::Crud;
            let repo = db.repository::<crate::domain::models::Message>();
            loop {
                let all = repo.list().await?;
                let mut children: Vec<_> = all
                    .into_iter()
                    .filter(|m| {
                        m.conversation_id == conversation_id
                            && m.parent_id.as_deref() == Some(&current_id)
                    })
                    .collect();
                children.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                if let Some(child) = children.first() {
                    current_id = child.id.clone();
                } else {
                    break;
                }
            }
        }
    }

    use quench_db::prelude::Crud;
    let conv_repo = db.repository::<crate::domain::models::Conversation>();
    if let Some(mut conv) = conv_repo.read(conversation_id).await? {
        conv.active_message_id = Some(current_id);
        conv.updated_at = chrono::Utc::now().to_rfc3339();
        conv_repo.update(&conv).await?;
    }

    Ok(())
}

#[derive(serde::Deserialize)]
pub struct RegenerateRequest {
    pub message_id: String,
}

#[post("/ui/chat/regenerate")]
pub async fn regenerate(
    Form(form): Form<RegenerateRequest>,
    Inject(state): Inject<ChatState>,
    Inject(db): Inject<Db>,
    Inject(switchboard): Inject<SwitchboardClient>,
) -> Response {
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::domain::models::Message>();
    let Ok(Some(msg)) = repo.read(&form.message_id).await else {
        return Response::new(StatusCode::NOT_FOUND);
    };

    if msg.role != "assistant" {
        return Response::text(
            StatusCode::BAD_REQUEST,
            "api_error_regenerate_non_assistant",
        );
    }

    let Some(parent_id) = msg.parent_id else {
        return Response::text(StatusCode::BAD_REQUEST, "api_error_no_parent_message");
    };

    let conv_repo = db.repository::<crate::domain::models::Conversation>();
    let project_id = match conv_repo.read(&msg.conversation_id).await {
        Ok(Some(conv)) => conv.project_id,
        _ => None,
    };

    let Ok(Some(parent_msg)) = repo.read(&parent_id).await else {
        return Response::text(StatusCode::NOT_FOUND, "api_error_parent_not_found");
    };

    let instances = switchboard.get_vllm_instances().await.unwrap_or_default();
    let Some(instance) = instances.iter().find(|i| i.is_chat_capable()) else {
        return Response::text(
            StatusCode::SERVICE_UNAVAILABLE,
            "api_error_no_models_available",
        );
    };

    let message_id = Uuid::new_v4().to_string();
    let req = ChatRequest {
        instance_id: instance.id.clone(),
        message: parent_msg.content,
        conversation_id: msg.conversation_id,
        project_id,
        parent_id: Some(parent_id),
        skip_user_message: true,
        search_provider: None,
        capability_profile: None,
        tool_confirmations: Vec::new(),
        file_ids: String::new(),
    };

    state.pending_messages.insert(message_id.clone(), req);

    let stream_url = with_base_path(&format!("/ui/chat/stream/{}", message_id));

    // 1. The thinking block for the message, under its new id.
    let ai_msg = div()
        .class("chat-message message-ai")
        .attr("id", format!("ai-{}", message_id))
        .attr("hx-ext", "sse")
        .attr("sse-connect", stream_url)
        .attr("sse-swap", "message")
        .child(
            div().class("message-inner").child(
                div()
                    .class("message-content")
                    .text("Sage is regenerating..."),
            ),
        );

    // 2. An OOB swap for the nav dot/tooltip IDs to match the new message id.
    let nav_update_oob = div().attr("hx-swap-oob", format!("outerHTML:#dot-ai-{}", form.message_id)).child(
        div()
            .class("nav-dot")
            .attr("id", format!("dot-ai-{}", message_id))
            .attr("data-msg-id", format!("ai-{}", message_id))
            .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
            .child(div().class("nav-tooltip").attr("id", format!("tooltip-ai-{}", message_id)).attr("data-i18n", "ui_chat_regenerating").text("Sage is regenerating...")),
    );

    // HX-Retarget tells htmx which element this response actually replaces.
    Response::html(
        StatusCode::OK,
        format!("{}{}", ai_msg.render(), nav_update_oob.render()),
    )
    .header("HX-Retarget", format!("#ai-{}", form.message_id))
    .header("HX-Reswap", "outerHTML")
}

#[get("/ui/chat/edit-form/{id}")]
pub async fn edit_form(Path(id): Path<String>, Inject(db): Inject<Db>) -> Response {
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::domain::models::Message>();
    let Ok(Some(msg)) = repo.read(&id).await else {
        return Response::new(StatusCode::NOT_FOUND);
    };

    let form = div().class("message-inner edit-mode").child(
        div().class("message-content").child(
            form()
                .attr("hx-post", with_base_path("/ui/chat/handle-edit"))
                .child(input().attr("type", "hidden").attr("name", "message_id").attr("value", &msg.id))
                .child(
                    textarea()
                        .class("edit-textarea")
                        .attr("name", "new_content")
                        .attr("onkeydown", "if(event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); this.form.dispatchEvent(new Event('submit', {bubbles: true, cancelable: true})); }")
                        .text(&msg.content),
                )
                .child(
                    div()
                        .class("edit-actions")
                        .child(button().attr("type", "button").class("branch-btn cancel-btn").attr("onclick", "window.location.reload();").attr("data-i18n", "ui_common_cancel").text("Cancel"))
                        .child(button().attr("type", "submit").class("branch-btn save-btn").attr("data-i18n", "ui_chat_save_submit").text("Save & Submit")),
                ),
        ),
    );

    Response::html(StatusCode::OK, form.render())
}

#[derive(serde::Deserialize)]
pub struct HandleEditRequest {
    pub message_id: String,
    pub new_content: String,
}

#[post("/ui/chat/handle-edit")]
pub async fn handle_edit(Form(form): Form<HandleEditRequest>, Inject(db): Inject<Db>) -> Response {
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::domain::models::Message>();
    let Ok(Some(msg)) = repo.read(&form.message_id).await else {
        return Response::new(StatusCode::NOT_FOUND);
    };

    let user_msg_id = Uuid::new_v4().to_string();
    let user_msg = crate::domain::models::Message {
        id: user_msg_id.clone(),
        conversation_id: msg.conversation_id.clone(),
        parent_id: msg.parent_id, // Branch from the same parent
        role: "user".to_string(),
        content: form.new_content.trim().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    if let Err(err) = repo.create(&user_msg).await {
        tracing::error!("Failed to create edited user message: {}", err);
        return Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal");
    }

    let conv_repo = db.repository::<crate::domain::models::Conversation>();
    if let Ok(Some(mut conv)) = conv_repo.read(&msg.conversation_id).await {
        conv.active_message_id = Some(user_msg_id);
        conv.updated_at = chrono::Utc::now().to_rfc3339();
        let _ = conv_repo.update(&conv).await;
    }

    // HX-Redirect to home, which will detect the user message at tip and auto-respond.
    let target_url = with_base_path(&format!("/ui/home?conversation_id={}", msg.conversation_id));
    Response::new(StatusCode::OK).header("HX-Redirect", target_url)
}

fn json_response(status: StatusCode, value: &serde_json::Value) -> Response {
    Response::json(status, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[get("/ui/chat/stats/{conversation_id}")]
pub async fn token_stats(
    claims: RequiredClaims,
    Path(conversation_id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(sage_config): Inject<SageConfig>,
) -> Response {
    if claims.or_401().is_err() {
        return Response::new(StatusCode::UNAUTHORIZED);
    }

    match crate::routers::ui::context_builder::build_conversation_context(
        &db,
        &conversation_id,
        4096, // Default context window
    )
    .await
    {
        Ok(ctx) => {
            let (_messages, usage) = crate::routers::ui::context_builder::get_context_for_llm(
                &ctx,
                &sage_config.system_prompt,
            );

            json_response(
                StatusCode::OK,
                &serde_json::json!({
                    "success": true,
                    "stats": usage.to_json(),
                    "display": usage.format_display(),
                    "warning": usage.warning_message(),
                }),
            )
        }
        Err(err) => json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &serde_json::json!({
                "success": false,
                "error": err,
            }),
        ),
    }
}

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn render_tool_result(
    tool_call: &crate::tools::ToolCall,
    result: &crate::tools::ToolResult,
) -> String {
    let tool_name = &tool_call.name;
    let query = tool_call
        .arguments
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("search");

    let icon = match tool_name.as_str() {
        "web_search" => "🔍",
        _ => "⚙️",
    };

    let css_class = if result.is_error {
        "tool-result tool-error"
    } else {
        "tool-result tool-success"
    };

    let content_html = if result.is_error {
        format!("<p>{}</p>", html_escape(&result.content))
    } else {
        use pulldown_cmark::{Parser, html};
        let parser = Parser::new(&result.content);
        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);
        html_output
    };

    // Don't capitalize tool name in header
    let header_text = if tool_name == "web_search" {
        format!("{} \"{}\"", tool_name, html_escape(query))
    } else {
        tool_name.to_string()
    };

    let content_html = content_html
        .replace("<h1>", "<h3>")
        .replace("</h1>", "</h3>");

    format!(
        r#"<div class="{}"><div class="tool-header"><span class="tool-icon">{}</span><span class="tool-name">{}</span></div><div class="tool-content">{}</div></div>"#,
        css_class, icon, header_text, content_html
    )
}

pub fn register_routes() {
    let _ = send_message as fn(_, _, _, _) -> _;
    let _ = stream_message as fn(_, _, _, _, _, _, _, _, _, _, _) -> _;
    let _ = delete_modal_empty as fn() -> _;
    let _ = delete_modal as fn(_, _, _) -> _;
    let _ = delete_conversation as fn(_, _, _) -> _;
    let _ = switch_branch as fn(_, _) -> _;
    let _ = regenerate as fn(_, _, _, _) -> _;
    let _ = edit_form as fn(_, _) -> _;
    let _ = handle_edit as fn(_, _) -> _;
    let _ = token_stats as fn(_, _, _, _) -> _;
}
