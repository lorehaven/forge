use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::{ChatMessage, VllmClient};
use crate::config::SageConfig;
use crate::routers::ui::common::format::format_message;
use crate::tools::ToolExecutor;
use actix_web::{HttpResponse, Responder, get, post, web};
use dashmap::DashMap;
use futures_util::StreamExt;
use quench_srv::prelude::{JwtConfig, with_base_path};
use quench_web::prelude::*;
use serde::Deserialize;
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
}

#[post("/send")]
pub async fn send_message(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    form: web::Form<ChatRequest>,
    state: web::Data<ChatState>,
) -> impl Responder {
    let _username = match quench_srv::actix::routers::ui::get_user_from_req(&req, &config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    let message_id = Uuid::new_v4().to_string();
    let mut chat_req = form.into_inner();
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
        // For regeneration, we don't show the user message again, just the thinking block
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

        return HttpResponse::Ok()
            .content_type("text/html")
            .body(ai_msg.render());
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
        .child(span().text(" Edit"));

    let user_msg = div()
        .class("chat-message message-user")
        .attr("id", format!("user-{}", message_id))
        .child(
            div()
                .class("message-inner")
                .raw()
                .text(format_message(&chat_req.message))
                .child(div().class("branch-controls").child(edit_btn)),
        );

    let ai_msg = div()
        .class("chat-message message-ai")
        .attr("id", format!("ai-{}", message_id))
        .attr("hx-ext", "sse")
        .attr("sse-connect", stream_url)
        .attr("sse-swap", "message")
        .child(
            div()
                .class("message-inner")
                .child(div().class("message-content").text("Sage is thinking...")),
        );

    let user_dot = div()
        .attr("hx-swap-oob", "beforeend:.chat-navigation")
        .child(
            div()
                .class("nav-dot")
                .attr("data-msg-id", format!("user-{}", message_id))
                .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
                .child(div().class("nav-tooltip").text(user_preview)),
        );

    let ai_dot = div()
        .attr("hx-swap-oob", "beforeend:.chat-navigation")
        .child(
            div()
                .class("nav-dot")
                .attr("id", format!("dot-ai-{}", message_id))
                .attr("data-msg-id", format!("ai-{}", message_id))
                .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
                .child(
                    div()
                        .class("nav-tooltip")
                        .attr("id", format!("tooltip-ai-{}", message_id))
                        .text("Sage is thinking..."),
                ),
        );

    HttpResponse::Ok().content_type("text/html").body(format!(
        "{}{}{}{}",
        user_msg.render(),
        ai_msg.render(),
        user_dot.render(),
        ai_dot.render()
    ))
}

fn encode_sse(event: &str, data: &str) -> actix_web::web::Bytes {
    let mut sse = format!("event: {}\n", event);
    for line in data.split('\n') {
        sse.push_str("data: ");
        sse.push_str(line);
        sse.push('\n');
    }
    sse.push('\n');
    actix_web::web::Bytes::from(sse)
}

/// Embed tool results into the response by replacing tool call markers
fn embed_tool_results_into_response(
    response: &str,
    tool_results_with_markers: Vec<(String, String)>, // (marker_text, result_html)
) -> String {
    let mut result = response.to_string();

    // Replace each tool call marker with its formatted result
    for (marker, html) in tool_results_with_markers {
        // Replace the marker with the formatted result
        result = result.replace(&marker, &format!("\n\n{}\n\n", html));
    }

    result
}

#[get("/stream/{id}")]
#[allow(clippy::too_many_arguments)]
pub async fn stream_message(
    id: web::Path<String>,
    req_http: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    state: web::Data<ChatState>,
    switchboard: web::Data<SwitchboardClient>,
    vllm: web::Data<VllmClient>,
    config: web::Data<SageConfig>,
    db: web::Data<quench_db::prelude::Db>,
    search_provider_registry: web::Data<std::sync::Arc<crate::tools::SearchProviderRegistry>>,
    metrics_collector: web::Data<std::sync::Arc<crate::metrics::MetricsCollector>>,
    rate_limiter: web::Data<std::sync::Arc<tokio::sync::Mutex<crate::rate_limiter::RateLimiter>>>,
    cost_tracker: web::Data<std::sync::Arc<crate::cost_tracking::CostTracker>>,
    response_cache: web::Data<crate::response_cache::ResponseCache>,
) -> impl Responder {
    let username =
        match quench_srv::actix::routers::ui::get_user_from_req(&req_http, &jwt_config).await {
            Some(claims) => claims.sub,
            None => return HttpResponse::Unauthorized().finish(),
        };

    let message_id = id.into_inner();

    let req = match state.pending_messages.get(&message_id) {
        Some(r) => r.clone(),
        None => return HttpResponse::NotFound().finish(),
    };

    tracing::info!(
        "Chat request: search_provider={:?}, instance_id={}",
        req.search_provider,
        req.instance_id
    );

    // Determine which capability profile to use
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

    // Create a tool registry with the active profile and request context
    let mut request_tool_registry = crate::tools::ToolRegistry::with_context(
        active_profile.clone(),
        Some(username.clone()),
        Some(req.conversation_id.clone()),
    );

    // Register all tool executors (mirroring the global registry initialization)
    request_tool_registry.register(
        "web_search".to_string(),
        Box::new(crate::tools::web_search::WebSearchExecutor::new(
            search_provider_registry.as_ref().clone(),
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
        "command".to_string(),
        Box::new(crate::tools::command::CommandExecutor::new()),
    );

    request_tool_registry.register(
        "code_executor".to_string(),
        Box::new(crate::tools::code_executor::CodeExecutor),
    );

    // Add tool confirmations from request
    if !req.tool_confirmations.is_empty() {
        let confirmations: Vec<&str> = req.tool_confirmations.iter().map(|s| s.as_str()).collect();
        request_tool_registry.add_confirmations(&confirmations);
    }

    // Set metrics collector, rate limiter, and cost tracker
    request_tool_registry.set_metrics_collector(metrics_collector.as_ref().clone());
    request_tool_registry.set_rate_limiter(rate_limiter.as_ref().clone());
    request_tool_registry.set_cost_tracker(cost_tracker.as_ref().clone());

    // Shadow the global tool_registry parameter with the request-specific one
    let tool_registry = web::Data::new(request_tool_registry);

    let instances = match switchboard.get_vllm_instances().await {
        Ok(i) => i,
        Err(err) => {
            tracing::error!("Failed to get vLLM instances: {}", err);
            return HttpResponse::InternalServerError().body(err.to_string());
        }
    };

    let Some(instance) = instances.into_iter().find(|i| i.id == req.instance_id) else {
        return HttpResponse::NotFound().body("Model instance not found");
    };

    // Check response cache
    let cache_key = crate::response_cache::ResponseCache::generate_key(
        &req.conversation_id,
        &req.message,
        &instance.model,
        req.search_provider
            .as_deref()
            .unwrap_or(&config.default_search_provider),
    );

    if let Some((cached, _)) = response_cache.get(&cache_key) {
        tracing::info!("Cache hit for message: {}", cache_key);
        let cached_html = div()
            .class("message-inner")
            .raw()
            .text(format_message(&cached.content))
            .render();
        let cached_response = format!(
            "data: {}\n\n",
            serde_json::json!({
                "type": "message",
                "content": cached_html,
                "cached": true
            })
        );
        return HttpResponse::Ok()
            .content_type("text/event-stream")
            .body(cached_response);
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
        msg.content.chars().count().div_ceil(3) + 4
    }

    let system_message = ChatMessage {
        role: "system".to_string(),
        content: config.system_prompt.clone(),
        tool_calls: None,
    };

    tracing::info!(
        "System prompt length: {} chars, contains AVAILABLE TOOLS: {}",
        system_message.content.len(),
        system_message.content.contains("AVAILABLE TOOLS")
    );
    if system_message.content.contains("websearch") {
        tracing::info!("✓ System prompt includes websearch tool definition");
    } else {
        tracing::warn!("✗ System prompt does NOT include websearch tool definition");
    }

    let current_user_message = ChatMessage {
        role: "user".to_string(),
        content: req.message.clone(),
        tool_calls: None,
    };

    tracing::info!("User message: {}", current_user_message.content);

    let system_tokens = estimate_tokens(&system_message);
    let current_user_tokens = estimate_tokens(&current_user_message);

    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::models::Conversation>();
    let mut active_message_id = None;
    let mut existing_title = None;
    if let Ok(Some(conv)) = repo.read(&req.conversation_id).await {
        active_message_id = conv.active_message_id;
        existing_title = Some(conv.title);
    }

    // Determine the base for history. If skip_user_message is true, we use req.parent_id as the base.
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

    let max_tokens = reserved_for_generation as u32;

    // Convert tool definitions to OpenAI format for vLLM
    // OpenAI format requires: {"type": "function", "function": {name, description, parameters}}
    let tool_definitions = tool_registry.get_definitions();
    let tools_json: Option<Vec<serde_json::Value>> = if !tool_definitions.is_empty() {
        let mut openai_tools = Vec::new();
        for tool_def in tool_definitions {
            // Nest the tool definition inside a "function" field
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
            return HttpResponse::InternalServerError().body(err.to_string());
        }
    };

    let mut full_content = String::new();
    let message_id_clone = message_id.clone();
    let db_clone = db.clone();
    let username_clone = username.clone();
    let tool_registry_clone = tool_registry.clone();

    let sse_stream = async_stream::stream! {
        let username = username_clone;
        let tool_registry = tool_registry_clone;
        let mut stream = stream;

        // =============================================================================
        // PHASE 1: Collect full response from vLLM without streaming
        // =============================================================================
        tracing::info!("[STREAM_REFACTOR] Phase 1: Collecting response");
        while let Some(res) = stream.next().await {
            match res {
                Ok(content) => {
                    full_content.push_str(&content);
                }
                Err(err) => {
                    let html = div()
                        .class("message-inner")
                        .child(div().class("message-content").text(format!("Error: {}", err)))
                        .render();
                    yield Ok::<_, actix_web::Error>(encode_sse("message", &html));
                    return;
                }
            }
        }

        tracing::info!("[STREAM_REFACTOR] Phase 1 complete: {} chars collected", full_content.len());

        // =============================================================================
        // PHASE 2: Parse tool calls and check for meta-questions
        // =============================================================================
        tracing::info!("[STREAM_REFACTOR] Phase 2: Parsing tool calls");
        let mut tool_calls = crate::tools::parser::parse_tool_calls(&full_content);
        tracing::info!("[STREAM_REFACTOR] Found {} tool calls", tool_calls.len());

        // Suppress if meta-question
        // Check if the user is asking meta-questions about tools
        let user_question_lower = req.message.to_lowercase();
        let is_meta_question = user_question_lower.contains("what tools")
            || user_question_lower.contains("what capabilities")
            || user_question_lower.contains("how do i use")
            || user_question_lower.contains("how do you use")
            || user_question_lower.contains("available tools")
            || user_question_lower.contains("can you do");

        if is_meta_question && !tool_calls.is_empty() {
            tracing::warn!("[STREAM_REFACTOR] Suppressing {} tool calls for meta-question", tool_calls.len());
            tool_calls.clear();
        }

        // =============================================================================
        // PHASE 3: Execute all tools and collect results with markers
        // =============================================================================
        tracing::info!("[STREAM_REFACTOR] Phase 3: Executing {} tools", tool_calls.len());
        let search_provider = req.search_provider.as_deref()
            .unwrap_or(&config.default_search_provider);

        // Map of marker string → formatted HTML result
        let mut tool_results_with_markers: Vec<(String, String)> = Vec::new();

        // Compile regex once outside the loop
        let tool_result_re = regex::Regex::new(r"<toolcall>.*?</toolcall>").ok();

        for tool_call in &tool_calls {
            let query = tool_call.arguments
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            tracing::info!("[STREAM_REFACTOR] Executing tool: {}", tool_call.name);

            // Execute the tool
            let result = if tool_call.name == "web_search" {
                let executor = crate::tools::web_search::WebSearchExecutor::new(
                    search_provider_registry.as_ref().clone()
                )
                .with_default_provider(search_provider.to_string());
                executor.execute(tool_call).await
            } else {
                tool_registry.execute(tool_call).await
            };

            // Format the result through vLLM
            let formatted_result = if !result.is_error {
                let parse_prompt = format!(
                    "Results for: {}\n\n{}\n\nProvide the information clearly and concisely. No headers or title needed—just the useful content.",
                    query,
                    result.content
                );

                let parse_messages = vec![
                    ChatMessage {
                        role: "system".to_string(),
                        content: "You are a helpful assistant. Present information clearly and concisely. Do not add titles, headers, or summaries—just the essential content.".to_string(),
                        tool_calls: None,
                    },
                    ChatMessage {
                        role: "user".to_string(),
                        content: parse_prompt,
                        tool_calls: None,
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

            // Find the tool call marker in the response
            // We need to locate the exact <toolcall>...</toolcall> string
            if let Some(re) = &tool_result_re {
                // Find ALL tool call markers in the response
                for marker_match in re.find_iter(&full_content) {
                    let marker_text = marker_match.as_str();
                    // Check if this marker matches our current tool call
                    // Match by tool name first, then by any content that appears in the arguments
                    let mut matches = marker_text.contains(&tool_call.name);

                    // Also try to match by the query/parameter value if available
                    if !matches && query != "unknown" {
                        matches = marker_text.contains(query);
                    }

                    // As fallback, check if any argument value appears in the marker
                    if !matches {
                        // Try to find any common values between arguments and marker
                        for (_, val) in tool_call.arguments.as_object().unwrap_or(&serde_json::Map::new()) {
                            if let Some(s) = val.as_str() && marker_text.contains(s) {
                                matches = true;
                                break;
                            }
                        }
                    }

                    if matches {
                        tool_results_with_markers.push((marker_text.to_string(), formatted_result.clone()));
                        tracing::info!("[STREAM_REFACTOR] Registered result for marker: {}", &marker_text[..marker_text.len().min(50)]);
                        break;
                    }
                }
            }
        }

        tracing::info!("[STREAM_REFACTOR] Phase 3 complete: {} tool results collected", tool_results_with_markers.len());

        // =============================================================================
        // PHASE 4: Embed tool results into response
        // =============================================================================
        tracing::info!("[STREAM_REFACTOR] Phase 4: Embedding results into response");

        // First, strip tool call markers from the raw response for storage
        let clean_content = crate::tools::parser::strip_tool_calls(&full_content);

        // Check if we have tool results before consuming tool_results_with_markers
        let has_tool_results = !tool_results_with_markers.is_empty();

        // Keep the tool results separately - we'll apply them after formatting
        let response_for_display = if has_tool_results {
            // Embed tool results into the full response (before formatting)
            embed_tool_results_into_response(&full_content, tool_results_with_markers)
        } else {
            full_content.clone()
        };

        tracing::info!("[STREAM_REFACTOR] Phase 4 complete: Response ready for database");

        // =============================================================================
        // PHASE 5: Database operations (unchanged from original)
        // =============================================================================
        use quench_db::prelude::Crud;
        let conv_repo = db_clone.repository::<crate::models::Conversation>();
        let updated_at = chrono::Utc::now().to_rfc3339();
        let title = if let Some(t) = existing_title {
            t
        } else {
            if req.message.chars().count() > 30 {
                format!("{}...", req.message.chars().take(30).collect::<String>())
            } else {
                req.message.clone()
            }
        };

        let mut conv = crate::models::Conversation {
            id: req.conversation_id.clone(),
            title,
            active_message_id: active_message_id.clone(),
            owner: username.clone(),
            project_id: req.project_id.clone(),
            updated_at,
        };

        let exists = conv_repo.read(&req.conversation_id).await.map(|o| o.is_some()).unwrap_or(false);
        if exists {
            if let Err(err) = conv_repo.update(&conv).await {
                tracing::error!("Failed to update conversation: {}", err);
            }
        } else {
            if let Err(err) = conv_repo.create(&conv).await {
                tracing::error!("Failed to create conversation: {}", err);
            }
        }

        let ai_parent_id;
        if !req.skip_user_message {
            // Create user message in DB
            let msg_repo = db_clone.repository::<crate::models::Message>();
            let user_msg_id = uuid::Uuid::new_v4().to_string();
            let user_msg = crate::models::Message {
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
            ai_parent_id = Some(user_msg_id);
        } else {
            ai_parent_id = req.parent_id.clone();
        }

        // Create AI message in DB with full response (including embedded tool results)
        let msg_repo = db_clone.repository::<crate::models::Message>();
        let ai_msg_id = uuid::Uuid::new_v4().to_string();
        let ai_msg = crate::models::Message {
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

        // Cache the response for future queries
        let cached_response = crate::response_cache::CachedResponse {
            content: response_for_display.trim().to_string(),
            model: instance.model.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            tokens_used: Some((full_content.len() / 3) as u32),
        };
        response_cache.set(cache_key, cached_response);
        tracing::info!("Cached response for future queries");

        // Update Conversation to point to the new tip
        conv.active_message_id = Some(ai_msg_id.clone());
        conv.updated_at = chrono::Utc::now().to_rfc3339();
        if let Err(err) = conv_repo.update(&conv).await {
            tracing::error!("Failed to update conversation tip: {}", err);
        }

        // =============================================================================
        // PHASE 6: Prepare and stream final SSE response (single yield)
        // =============================================================================
        tracing::info!("[STREAM_REFACTOR] Phase 6: Preparing final SSE response");

        // Format the text content, but preserve embedded tool results as raw HTML
        let final_rendered = if has_tool_results {
            // Apply simple markdown formatting (backticks, bold, italic) to text while preserving tool result HTML
            let mut formatted_response = response_for_display.clone();

            // Format backticks as inline code (but not inside tool-result divs)
            if let Ok(backtick_re) = regex::Regex::new(r"`([^`]+)`") {
                formatted_response = backtick_re.replace_all(&formatted_response, "<code>$1</code>").to_string();
            }

            // Format **bold**
            if let Ok(bold_re) = regex::Regex::new(r"\*\*([^\*]+)\*\*") {
                formatted_response = bold_re.replace_all(&formatted_response, "<strong>$1</strong>").to_string();
            }

            // Format *italic*
            if let Ok(italic_re) = regex::Regex::new(r"\*([^\*]+)\*") {
                formatted_response = italic_re.replace_all(&formatted_response, "<em>$1</em>").to_string();
            }

            format!("<div class=\"message-content\">{}</div>", formatted_response.trim())
        } else {
            format_message(clean_content.trim())
        };

        // Fetch siblings for the new AI message to show branch controls if needed
        let mut controls = div().class("branch-controls");
        if let Ok(siblings) = get_siblings(&db_clone, &req.conversation_id, ai_parent_id.as_deref()).await {
            let total_siblings = siblings.len();
            let sibling_index = siblings.iter().position(|s| s.id == ai_msg_id).unwrap_or(0);

            if total_siblings > 1 {
                let prev_index = if sibling_index == 0 { total_siblings - 1 } else { sibling_index - 1 };
                let next_index = if sibling_index == total_siblings - 1 { 0 } else { sibling_index + 1 };
                let prev_sibling = &siblings[prev_index];
                let next_sibling = &siblings[next_index];

                let nav = div()
                    .class("branch-nav")
                    .child(
                        form()
                            .attr("hx-post", with_base_path("/ui/chat/conversations/switch"))
                            .attr("style", "display: inline;")
                            .child(input().attr("type", "hidden").attr("name", "conversation_id").attr("value", &req.conversation_id))
                            .child(input().attr("type", "hidden").attr("name", "target_message_id").attr("value", &prev_sibling.id))
                            .child(
                                button()
                                    .class("branch-btn")
                                    .attr("type", "submit")
                                    .child(i().class("fas fa-chevron-left"))
                            )
                    )
                    .child(
                        span()
                            .class("branch-info")
                            .text(format!("{}/{}", sibling_index + 1, total_siblings))
                    )
                    .child(
                        form()
                            .attr("hx-post", with_base_path("/ui/chat/conversations/switch"))
                            .attr("style", "display: inline;")
                            .child(input().attr("type", "hidden").attr("name", "conversation_id").attr("value", &req.conversation_id))
                            .child(input().attr("type", "hidden").attr("name", "target_message_id").attr("value", &next_sibling.id))
                            .child(
                                button()
                                    .class("branch-btn")
                                    .attr("type", "submit")
                                    .child(i().class("fas fa-chevron-right"))
                            )
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
            .child(span().text(" Regenerate"));

        controls = controls.child(regenerate_btn);

        let message_inner = div()
            .class("message-inner")
            .raw()
            .text(&final_rendered);

        let oob_transition = div()
            .class("chat-message message-ai")
            .attr("id", format!("ai-{}", ai_msg_id))
            .attr("hx-swap-oob", format!("outerHTML:#ai-{}", message_id_clone))
            .child(
                message_inner.child(div()
                    .class("branch-controls")
                    .raw()
                    .text(controls.render())
                )
            );

        // 2. Transition the navigation dot and tooltip IDs for the AI message
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

        // 3. Transition the USER message block to its permanent ID and add the Edit button
        let mut user_oob_transition = String::new();
        let mut user_nav_dot_transition = String::new();

        if let Some(ref uid) = ai_parent_id
            && !req.skip_user_message {
                let edit_btn = button()
                    .class("branch-btn edit-btn")
                    .attr("hx-get", with_base_path(&format!("/ui/chat/edit-form/{}", uid)))
                    .attr("hx-target", format!("#user-{}", uid))
                    .attr("hx-swap", "innerHTML")
                    .child(i().class("fas fa-edit"))
                    .child(span().text(" Edit"));

                let user_controls = div()
                    .class("branch-controls")
                    .child(edit_btn);

                user_oob_transition = div()
                    .class("chat-message message-user")
                    .attr("id", format!("user-{}", uid))
                    .attr("hx-swap-oob", format!("outerHTML:#user-{}", message_id_clone))
                    .child(
                        div()
                            .class("message-inner")
                            .raw()
                            .text(format_message(&req.message))
                            .child(user_controls)
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
            let project_repo = db_clone.repository::<crate::models::Project>();
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
                                .child(span().text("Projects"))
                        )
                        .child(
                            button()
                                .class("branch-btn")
                                .attr("onclick", "event.stopPropagation();")
                                .attr("hx-get", with_base_path("/ui/projects/new-modal"))
                                .attr("hx-target", "body")
                                .attr("hx-swap", "beforeend")
                                .child(i().class("fas fa-plus"))
                                .child(span().text("New")),
                        ),
                );

                let mut projects_content = div().class("history-section-content");
                if !has_projects {
                    projects_content = projects_content.class("hidden");
                }

                for project in &projects {
                    let is_active = Some(project.id.clone()) == req.project_id;
                    let item_class = if is_active { "history-item active project-item" } else { "history-item project-item" };
                    let link_class = if is_active { "history-item-link active" } else { "history-item-link" };
                    let icon_class = if is_active { "fas fa-folder-open" } else { "fas fa-folder" };

                    let item = div().class(item_class).child(
                        a().class(link_class)
                            .attr("href", with_base_path(&format!("/ui/home?project_id={}", project.id)))
                            .child(i().class(icon_class).attr("style", "margin-right: 8px;"))
                            .child(span().text(&project.name)),
                    );
                    projects_content = projects_content.child(item);

                    let project_convs: Vec<_> = conversations.iter().filter(|c| c.project_id.as_deref() == Some(&project.id)).collect();
                    for conv_item in project_convs {
                        let is_conv_active = conv_item.id == req.conversation_id;
                        let conv_item_class = if is_conv_active { "history-item active project-conv-item" } else { "history-item project-conv-item" };
                        let conv_link_class = if is_conv_active { "history-item-link active" } else { "history-item-link" };
                        let item_id = format!("history-item-{}", conv_item.id);
                        let conv_url = format!("/ui/home?conversation_id={}&project_id={}", conv_item.id, project.id);

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
                                    .child(i().class("fas fa-trash")).child(span().text("Delete"))
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
                            .child(span().text(conv_header_text))
                    )
            );

            let mut global_content = div().class("history-section-content");

            let global_convs: Vec<_> = conversations.iter().filter(|c| c.project_id.is_none()).collect();
            for conv_item in global_convs {
                let is_active = conv_item.id == req.conversation_id;
                let item_class = if is_active { "history-item active" } else { "history-item" };
                let link_class = if is_active { "history-item-link active" } else { "history-item-link" };
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
                                            .child(span().text("Delete"))
                                    )
                            )
                    );
                global_content = global_content.child(item);
            }
            history_list = history_list.child(global_content);
            final_payload.push_str(&history_list.render());
        }

        tracing::info!("[STREAM_REFACTOR] Phase 6 complete: Yielding final response");
        yield Ok::<_, actix_web::Error>(encode_sse("message", &final_payload));
        state.pending_messages.remove(&message_id_clone);
    };

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(sse_stream)
}

#[derive(serde::Deserialize)]
pub struct DeleteQuery {
    pub active_id: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct DeleteModalQuery {
    pub active_id: Option<String>,
}

#[get("/conversations/delete-modal/empty")]
pub async fn delete_modal_empty() -> impl Responder {
    let empty = div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal");
    HttpResponse::Ok()
        .content_type("text/html")
        .body(empty.render())
}

#[get("/conversations/delete-modal/{id}")]
pub async fn delete_modal(
    id: web::Path<String>,
    query: web::Query<DeleteModalQuery>,
    db: web::Data<quench_db::prelude::Db>,
) -> impl Responder {
    let conv_id = id.into_inner();
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::models::Conversation>();

    let mut title = "this conversation".to_string();
    if let Ok(Some(conv)) = repo.read(&conv_id).await {
        title = format!("\"{}\"", conv.title);
    }

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
                        .child(div().class("estimates-modal-title").text("Confirm Delete"))
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
                        .child(p().text("Are you sure you want to delete this conversation?"))
                        .child(div().class("model-to-delete-name").text(title))
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
                                        .text("Cancel"),
                                )
                                .child(
                                    button()
                                        .class("button danger")
                                        .attr("type", "submit")
                                        .text("Delete"),
                                ),
                        ),
                ),
        );

    HttpResponse::Ok()
        .content_type("text/html")
        .body(modal.render())
}

#[post("/conversations/delete/{id}")]
pub async fn delete_conversation(
    id: web::Path<String>,
    query: web::Query<DeleteQuery>,
    db: web::Data<quench_db::prelude::Db>,
) -> impl Responder {
    let id_str = id.into_inner();
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::models::Conversation>();
    let _ = repo.delete(&id_str).await;

    let mut response = HttpResponse::Ok();
    if query.active_id.as_deref() == Some(&id_str) {
        response.append_header(("HX-Redirect", with_base_path("/ui/home")));
        return response.body("");
    }

    let close_modal = div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal")
        .render();
    let oob_delete = div()
        .attr("id", format!("history-item-{}", id_str))
        .attr("hx-swap-oob", "delete")
        .render();

    response
        .content_type("text/html")
        .body(format!("{}{}", close_modal, oob_delete))
}

#[derive(serde::Deserialize)]
pub struct SwitchBranchRequest {
    pub conversation_id: String,
    pub target_message_id: String,
}

#[post("/conversations/switch")]
pub async fn switch_branch(
    form: web::Form<SwitchBranchRequest>,
    db: web::Data<quench_db::prelude::Db>,
) -> impl Responder {
    if let Err(err) =
        switch_active_message(&db, &form.conversation_id, &form.target_message_id).await
    {
        tracing::error!("Failed to switch active branch: {}", err);
        return HttpResponse::InternalServerError().body(err.to_string());
    }

    HttpResponse::Ok()
        .append_header((
            "HX-Redirect",
            with_base_path(&format!(
                "/ui/home?conversation_id={}",
                form.conversation_id
            )),
        ))
        .body("")
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
                SELECT role, content FROM thread ORDER BY depth DESC",
                table, table
            );

            let rows = sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(query.as_str()))
                .bind(current_id)
                .fetch_all(pg_db.pool())
                .await?;

            for (role, content) in rows {
                chat_messages.push(ChatMessage {
                    role,
                    content,
                    tool_calls: None,
                });
            }
        }
        quench_db::prelude::Db::InMemory(_mem_db) => {
            use quench_db::prelude::Crud;
            let repo = db.repository::<crate::models::Message>();
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
                });
            }
        }
    }

    Ok(chat_messages)
}

pub async fn get_conversation_message_nodes(
    db: &quench_db::prelude::Db,
    active_message_id: Option<&str>,
) -> Result<Vec<crate::models::Message>, anyhow::Error> {
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
                message_nodes.push(crate::models::Message {
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
            let repo = db.repository::<crate::models::Message>();
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
) -> Result<Vec<crate::models::Message>, anyhow::Error> {
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
                siblings.push(crate::models::Message {
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
            let repo = db.repository::<crate::models::Message>();
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
            let repo = db.repository::<crate::models::Message>();
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
    let conv_repo = db.repository::<crate::models::Conversation>();
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

#[post("/regenerate")]
pub async fn regenerate(
    form: web::Form<RegenerateRequest>,
    state: web::Data<ChatState>,
    db: web::Data<quench_db::prelude::Db>,
    switchboard: web::Data<SwitchboardClient>,
) -> impl Responder {
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::models::Message>();
    let Ok(Some(msg)) = repo.read(&form.message_id).await else {
        return HttpResponse::NotFound().finish();
    };

    if msg.role != "assistant" {
        return HttpResponse::BadRequest().body("Can only regenerate assistant messages");
    }

    let Some(parent_id) = msg.parent_id else {
        return HttpResponse::BadRequest().body("Message has no parent to regenerate from");
    };

    let conv_repo = db.repository::<crate::models::Conversation>();
    let project_id = match conv_repo.read(&msg.conversation_id).await {
        Ok(Some(conv)) => conv.project_id,
        _ => None,
    };

    let Ok(Some(parent_msg)) = repo.read(&parent_id).await else {
        return HttpResponse::NotFound().body("Parent message not found");
    };

    // Use current models from switchboard
    let instances = switchboard.get_vllm_instances().await.unwrap_or_default();
    let Some(instance) = instances.first() else {
        return HttpResponse::ServiceUnavailable().body("No AI models available for regeneration");
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
    };

    state.pending_messages.insert(message_id.clone(), req);

    let stream_url = with_base_path(&format!("/ui/chat/stream/{}", message_id));

    // 1. The thinking block for the message itself (now with the NEW ID)
    let ai_msg = div()
        .class("chat-message message-ai")
        .attr("id", format!("ai-{}", message_id)) // NEW ID
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

    // 2. An OOB swap to update the navigation dot and tooltip IDs to match the NEW message ID
    let nav_update_oob = div()
        .attr("hx-swap-oob", format!("outerHTML:#dot-ai-{}", form.message_id))
        .child(
            div()
                .class("nav-dot")
                .attr("id", format!("dot-ai-{}", message_id))
                .attr("data-msg-id", format!("ai-{}", message_id))
                .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
                .child(
                    div()
                        .class("nav-tooltip")
                        .attr("id", format!("tooltip-ai-{}", message_id))
                        .text("Sage is regenerating..."),
                ),
        );

    // We use HX-Target to tell HTMX to replace the specific element
    HttpResponse::Ok()
        .content_type("text/html")
        .append_header(("HX-Retarget", format!("#ai-{}", form.message_id)))
        .append_header(("HX-Reswap", "outerHTML"))
        .body(format!("{}{}", ai_msg.render(), nav_update_oob.render()))
}

#[get("/edit-form/{id}")]
pub async fn edit_form(
    id: web::Path<String>,
    db: web::Data<quench_db::prelude::Db>,
) -> impl Responder {
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::models::Message>();
    let Ok(Some(msg)) = repo.read(&id).await else {
        return HttpResponse::NotFound().finish();
    };

    let form = div()
        .class("message-inner edit-mode")
        .child(
            div()
                .class("message-content")
                .child(
                    form()
                        .attr("hx-post", with_base_path("/ui/chat/handle-edit"))
                        .child(input().attr("type", "hidden").attr("name", "message_id").attr("value", &msg.id))
                        .child(
                            textarea()
                                .class("edit-textarea")
                                .attr("name", "new_content")
                                .attr("onkeydown", "if(event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); this.form.dispatchEvent(new Event('submit', {bubbles: true, cancelable: true})); }")
                                .text(&msg.content)
                        )
                        .child(
                            div()
                                .class("edit-actions")
                                .child(
                                    button()
                                        .attr("type", "button")
                                        .class("branch-btn cancel-btn")
                                        .attr("onclick", "window.location.reload();")
                                        .text("Cancel")
                                )
                                .child(
                                    button()
                                        .attr("type", "submit")
                                        .class("branch-btn save-btn")
                                        .text("Save & Submit")
                                )
                        )
                )
        );

    HttpResponse::Ok()
        .content_type("text/html")
        .body(form.render())
}

#[derive(serde::Deserialize)]
pub struct HandleEditRequest {
    pub message_id: String,
    pub new_content: String,
}

#[post("/handle-edit")]
pub async fn handle_edit(
    form: web::Form<HandleEditRequest>,
    db: web::Data<quench_db::prelude::Db>,
) -> impl Responder {
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::models::Message>();
    let Ok(Some(msg)) = repo.read(&form.message_id).await else {
        return HttpResponse::NotFound().finish();
    };

    let user_msg_id = Uuid::new_v4().to_string();
    let user_msg = crate::models::Message {
        id: user_msg_id.clone(),
        conversation_id: msg.conversation_id.clone(),
        parent_id: msg.parent_id, // Branch from the same parent
        role: "user".to_string(),
        content: form.new_content.trim().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    if let Err(err) = repo.create(&user_msg).await {
        tracing::error!("Failed to create edited user message: {}", err);
        return HttpResponse::InternalServerError().body(err.to_string());
    }

    // Update conversation tip to the new user message
    let conv_repo = db.repository::<crate::models::Conversation>();
    if let Ok(Some(mut conv)) = conv_repo.read(&msg.conversation_id).await {
        conv.active_message_id = Some(user_msg_id);
        conv.updated_at = chrono::Utc::now().to_rfc3339();
        let _ = conv_repo.update(&conv).await;
    }

    // Redirect to home page which will now detect the user message at tip and auto-respond
    // We use HX-Redirect to tell HTMX to do a full page transition
    let target_url = with_base_path(&format!("/ui/home?conversation_id={}", msg.conversation_id));
    HttpResponse::Ok()
        .append_header(("HX-Redirect", target_url))
        .finish()
}

#[get("/stats/{conversation_id}")]
pub async fn token_stats(
    conversation_id: web::Path<String>,
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<quench_db::prelude::Db>,
    sage_config: web::Data<crate::config::SageConfig>,
) -> impl Responder {
    // Check auth
    if quench_srv::actix::routers::ui::get_user_from_req(&req, &config)
        .await
        .is_none()
    {
        return HttpResponse::Unauthorized().finish();
    }

    let conv_id = conversation_id.into_inner();

    // Build conversation context
    match crate::routers::ui::context_builder::build_conversation_context(
        &db, &conv_id, 4096, // Default context window
    )
    .await
    {
        Ok(ctx) => {
            let (_messages, usage) = crate::routers::ui::context_builder::get_context_for_llm(
                &ctx,
                &sage_config.system_prompt,
            );

            HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "stats": usage.to_json(),
                "display": usage.format_display(),
                "warning": usage.warning_message(),
            }))
        }
        Err(err) => HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "error": err,
        })),
    }
}

pub fn scope() -> actix_web::Scope {
    web::scope("/chat")
        .service(send_message)
        .service(stream_message)
        .service(token_stats)
        .service(delete_conversation)
        .service(delete_modal)
        .service(delete_modal_empty)
        .service(switch_branch)
        .service(regenerate)
        .service(edit_form)
        .service(handle_edit)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn render_tool_result(
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
        // Parse markdown content to HTML
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

    // Convert h1 to h3 in tool content
    let content_html = content_html
        .replace("<h1>", "<h3>")
        .replace("</h1>", "</h3>");

    // Tool results are appended inside the message content area
    format!(
        r#"<div class="{}"><div class="tool-header"><span class="tool-icon">{}</span><span class="tool-name">{}</span></div><div class="tool-content">{}</div></div>"#,
        css_class, icon, header_text, content_html
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_db::prelude::{Crud, Db};

    #[actix_web::test]
    async fn retrieves_only_the_selected_conversation_branch() {
        let db = Db::InMemory(quench_db::InMemoryDb::new());
        let repo = db.repository::<crate::models::Message>();

        for message in [
            crate::models::Message {
                id: "root".to_string(),
                conversation_id: "conversation".to_string(),
                parent_id: None,
                role: "user".to_string(),
                content: "question".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
            crate::models::Message {
                id: "answer-a".to_string(),
                conversation_id: "conversation".to_string(),
                parent_id: Some("root".to_string()),
                role: "assistant".to_string(),
                content: "answer a".to_string(),
                created_at: "2026-01-01T00:00:01Z".to_string(),
            },
            crate::models::Message {
                id: "answer-b".to_string(),
                conversation_id: "conversation".to_string(),
                parent_id: Some("root".to_string()),
                role: "assistant".to_string(),
                content: "answer b".to_string(),
                created_at: "2026-01-01T00:00:02Z".to_string(),
            },
        ] {
            repo.create(&message).await.unwrap();
        }

        let branch = get_conversation_messages(&db, Some("answer-b"))
            .await
            .unwrap();
        let siblings = get_siblings(&db, "conversation", Some("root"))
            .await
            .unwrap();

        assert_eq!(
            branch
                .iter()
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>(),
            vec!["question", "answer b"]
        );
        assert_eq!(
            siblings
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["answer-a", "answer-b"]
        );
    }
}
