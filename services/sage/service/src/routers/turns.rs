//! A chat turn.
//!
//! The request that starts it, the stream that carries the answer to the browser, and the work in between (planning, searching,
//! the model, tools, storing the messages). The stream is plain server-sent events whose names and JSON are
//! [`sage_core::StreamEvent`]; nothing on it is markup.

use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::{ChatMessage, VllmClient};
use crate::config::SageConfig;
use crate::routers::conversations::get_conversation_messages;
use crate::routers::files::Username;
use crate::tools::ToolExecutor;
use quench_core::bytes::Bytes;
use quench_core::stream::StreamExt;
use quench_core::uuid::Uuid;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Json, Path, Response, get, http::StatusCode, post};
use quench_serde::Deserialize;
use sage_core::{Done, Status, StreamEvent, TurnStarted};
use std::sync::Arc;

fn estimate_tokens(msg: &ChatMessage) -> usize {
    let image_tokens = msg.images.as_ref().map_or(0, |imgs| {
        imgs.len() * crate::files::images::image_token_estimate()
    });
    msg.content.chars().count().div_ceil(3) + 4 + image_tokens
}

#[derive(Debug)]
pub struct ChatState {
    pub pending_messages: quench_core::map::ShardedMap<String, ChatRequest>,
}

#[derive(Debug, Deserialize, Clone)]
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
    #[must_use]
    pub fn file_id_list(&self) -> Vec<String> {
        self.file_ids
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(std::string::ToString::to_string)
            .collect()
    }
}

/// An event as the bytes the stream carries.
fn event(event: &StreamEvent) -> Bytes {
    Bytes::from(event.frame())
}

/// The SSE `status` event for a grounding stage.
#[must_use]
pub fn stage_event(stage: &crate::grounding::Stage) -> Bytes {
    use crate::grounding::Stage;
    let (stage, detail) = match stage {
        Stage::Planning => ("planning", None),
        Stage::Searching(queries) => ("searching", Some(queries.join(" · "))),
        Stage::Reading(_) => ("reading", None),
    };
    event(&StreamEvent::Status(Status {
        stage: stage.to_string(),
        detail,
    }))
}

/// The `failure` event sent in place of the answer.
fn error_event(err: &dyn std::fmt::Display) -> Bytes {
    event(&StreamEvent::Failure(err.to_string()))
}

/// Embed tool results into the response by replacing tool call markers
#[must_use]
pub fn embed_tool_results_into_response(
    response: &str,
    tool_results_with_markers: Vec<(String, String)>, // (marker_text, result_html)
) -> String {
    let mut result = response.to_string();

    quench_log::info!(
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
        quench_log::info!(
            "[EMBED] Looking for marker: found={}, marker_preview: {}",
            found,
            marker_preview
        );

        let before_len = result.len();
        result = result.replace(&marker, &format!("\n\n{html}\n\n"));
        let after_len = result.len();

        quench_log::info!(
            "[EMBED] After replace: content_len_change={}",
            i64::try_from(after_len).unwrap_or(i64::MAX) - i64::try_from(before_len).unwrap_or(0)
        );
    }

    quench_log::info!(
        "[EMBED] Embedding complete: final_response_len={}",
        result.len()
    );
    result
}

/// Whether a client-requested profile only narrows the operator's: every tool it enables is one
/// the configured profile already enables.
#[must_use]
pub fn narrows(
    requested: &crate::tools::capabilities::CapabilityProfile,
    configured: &crate::tools::capabilities::CapabilityProfile,
) -> bool {
    requested.enabled_tools.is_subset(&configured.enabled_tools)
}

/// The capability profile the request asks for, if it is no broader than the configured one; else
/// the configured one. The profile is the operator's decision: a caller can drop tools, not add them.
fn resolve_profile(
    req: &ChatRequest,
    config: &SageConfig,
) -> crate::tools::capabilities::CapabilityProfile {
    req.capability_profile.as_ref().map_or_else(
        || config.capability_profile.clone(),
        |requested_profile| {
            crate::tools::capabilities::get_profile(requested_profile)
                .filter(|profile| narrows(profile, &config.capability_profile))
                .map_or_else(
                    || {
                        quench_log::warn!(
                            "Requested profile '{}' unknown or broader than '{}', using the latter",
                            requested_profile,
                            config.capability_profile.name
                        );
                        config.capability_profile.clone()
                    },
                    |profile| {
                        quench_log::info!(
                            "Using requested capability profile: {}",
                            requested_profile
                        );
                        profile
                    },
                )
        },
    )
}

/// What the per-request tool registry shares with the rest of the service.
struct RegistryDeps<'a> {
    search_provider_registry: &'a Arc<crate::tools::SearchProviderRegistry>,
    db: &'a Db,
    switchboard: &'a SwitchboardClient,
    vllm: &'a VllmClient,
    metrics_collector: &'a Arc<crate::observability::metrics::MetricsCollector>,
    rate_limiter: &'a Arc<tokio::sync::Mutex<crate::runtime::rate_limiter::RateLimiter>>,
    cost_tracker: &'a Arc<crate::observability::cost_tracking::CostTracker>,
}

/// The tools one request may call, scoped to its user and conversation.
fn build_tool_registry(
    active_profile: &crate::tools::capabilities::CapabilityProfile,
    username: &str,
    req: &ChatRequest,
    deps: &RegistryDeps<'_>,
) -> crate::tools::ToolRegistry {
    let mut registry = crate::tools::ToolRegistry::with_context(
        active_profile.clone(),
        Some(username.to_string()),
        Some(req.conversation_id.clone()),
    );

    // Mirrors the global registry's own initialization.
    registry.register(
        "web_search".to_string(),
        Box::new(crate::tools::web_search::WebSearchExecutor::new(
            deps.search_provider_registry.clone(),
        )),
    );

    registry.register(
        "calculator".to_string(),
        Box::new(crate::tools::calculator::CalculatorExecutor),
    );

    registry.register(
        "web_fetch".to_string(),
        Box::new(crate::tools::web_fetch::WebFetchExecutor::new()),
    );

    registry.register(
        "file_ops".to_string(),
        Box::new(crate::tools::file_ops::FileOpsExecutor::from_env()),
    );

    registry.register(
        "file_search".to_string(),
        Box::new(crate::tools::file_search::FileSearchExecutor::new(
            (*deps.db).clone(),
            (*deps.switchboard).clone(),
            (*deps.vllm).clone(),
            Some(req.conversation_id.clone()),
            req.project_id.clone(),
        )),
    );

    registry.register(
        "file_list".to_string(),
        Box::new(crate::tools::file_list::FileListExecutor::new(
            (*deps.db).clone(),
            Some(req.conversation_id.clone()),
            req.project_id.clone(),
        )),
    );

    registry.register(
        "command".to_string(),
        Box::new(crate::tools::command::CommandExecutor::new()),
    );

    registry.register(
        "code_executor".to_string(),
        Box::new(crate::tools::code_executor::CodeExecutor),
    );

    if !req.tool_confirmations.is_empty() {
        let confirmations: Vec<&str> = req
            .tool_confirmations
            .iter()
            .map(std::string::String::as_str)
            .collect();
        registry.add_confirmations(&confirmations);
    }

    registry.set_metrics_collector(deps.metrics_collector.clone());
    registry.set_rate_limiter(deps.rate_limiter.clone());
    registry.set_cost_tracker(deps.cost_tracker.clone());

    registry
}

/// The turn was abandoned: the client went away, or an error has already been sent to it.
struct Aborted;

type Tx = tokio::sync::mpsc::Sender<Bytes>;

/// How the model's context splits between the prompt and the answer.
struct PromptLimits {
    prompt_budget: usize,
    reserved_for_generation: usize,
}

impl PromptLimits {
    const fn for_context(max_model_len: usize) -> Self {
        let reserved_for_generation = if max_model_len > 4096 {
            2048
        } else if max_model_len > 2048 {
            1024
        } else {
            512
        };
        Self {
            prompt_budget: max_model_len.saturating_sub(reserved_for_generation),
            reserved_for_generation,
        }
    }
}

/// What is already stored of the conversation a message is added to.
struct StoredConversation {
    active_message_id: Option<String>,
    title: Option<String>,
    project_id: Option<String>,
}

/// The web sources a grounded answer was built from.
#[derive(Default)]
struct GroundingState {
    web_sources: Vec<crate::grounding::WebSource>,
    injected: bool,
    evidence: String,
    question: String,
}

/// Where a turn's messages ended up in the database.
struct Persisted {
    ai_msg_id: String,
    ai_parent_id: Option<String>,
    title: String,
}

/// Everything one streamed answer needs, moved into the task that produces it.
struct Turn {
    chat_state: Arc<ChatState>,
    message_id: String,
    req: ChatRequest,
    username: String,
    db: Arc<Db>,
    vllm: Arc<VllmClient>,
    switchboard: Arc<SwitchboardClient>,
    config: Arc<SageConfig>,
    search_provider_registry: Arc<crate::tools::SearchProviderRegistry>,
    tool_registry: Arc<crate::tools::ToolRegistry>,
    active_profile: crate::tools::capabilities::CapabilityProfile,
    instance: crate::clients::switchboard::VllmInstance,
    limits: PromptLimits,
    system_message: ChatMessage,
    user_message: ChatMessage,
    user_tokens: usize,
    history: Vec<ChatMessage>,
    injected_rag_hits: Vec<crate::files::rag::ChunkHit>,
    stored: StoredConversation,
    today: String,
}

/// The `OpenAI` tool-call definitions of the tools the request may use.
fn openai_tools(tool_registry: &crate::tools::ToolRegistry) -> Option<Vec<quench_serde::Value>> {
    let tool_definitions = tool_registry.get_definitions();
    if tool_definitions.is_empty() {
        return None;
    }
    // OpenAI tool-call format: {"type": "function", "function": {name, description, parameters}}
    let openai_tools: Vec<quench_serde::Value> = tool_definitions
        .into_iter()
        .map(|tool_def| {
            quench_serde::json!({
                "type": "function",
                "function": {
                    "name": tool_def.name,
                    "description": tool_def.description,
                    "parameters": tool_def.parameters
                }
            })
        })
        .collect();
    quench_log::info!(
        "[VLLM_REQUEST] Sending {} tools to vLLM in OpenAI format",
        openai_tools.len()
    );
    Some(openai_tools)
}

/// Whether the user is asking about the tools rather than asking for one to be used.
fn is_meta_question(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "what tools",
        "what capabilities",
        "how do i use",
        "how do you use",
        "available tools",
        "can you do",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

/// The first `<tool_call>` block in the answer that the call refers to, by tool name or argument.
fn marker_for_call(
    tool_call: &crate::tools::ToolCall,
    query: &str,
    full_content: &str,
    tool_result_re: &quench_regex::Regex,
) -> Option<String> {
    tool_result_re
        .find_iter(full_content)
        .map(|marker_match| marker_match.as_str())
        .find(|marker_text| {
            // Match by tool name first, then by argument content.
            marker_text.contains(&tool_call.name)
                || (query != "unknown" && marker_text.contains(query))
                // Fallback: any argument value appearing in the marker counts too.
                || tool_call.arguments.as_object().is_some_and(|arguments| {
                    arguments
                        .values()
                        .filter_map(quench_serde::Value::as_str)
                        .any(|s| marker_text.contains(s))
                })
        })
        .map(str::to_string)
}

/// The sidebar-style status line for a conversation's first 30 characters, with an ellipsis.
fn preview(text: &str) -> String {
    let head: String = text.chars().take(30).collect();
    if text.chars().count() > 30 {
        format!("{head}...")
    } else {
        head
    }
}

impl Turn {
    /// Runs the turn, sending the answer to the browser as it is written; a turn that aborts has
    /// already told the client what it can.
    async fn run(mut self, tx: Tx) {
        if self.run_inner(&tx).await.is_ok() {
            self.chat_state.pending_messages.remove(&self.message_id);
        }
    }

    async fn run_inner(&mut self, tx: &Tx) -> Result<(), Aborted> {
        // Small models can't be trusted to decide when to search, so in grounded mode the
        // harness plans, searches and fetches up front and injects numbered sources.
        let grounding_cfg = crate::grounding::GroundingConfig::from_env();
        let grounded = grounding_cfg.enabled
            && self
                .active_profile
                .is_enabled(crate::tools::Tool::WebSearch);
        let grounding = if grounded {
            self.gather_grounding(&grounding_cfg, tx).await?
        } else {
            GroundingState::default()
        };

        let messages = self.assemble_messages();
        // Grounded mode skips native tool calls: the harness already retrieved, and the launch
        // config may not enable auto tool choice (vLLM rejects `tools` then).
        let tools_json = if grounded {
            None
        } else {
            openai_tools(&self.tool_registry)
        };

        let streamed = self.stream_answer(messages, tools_json, tx).await?;
        let mut full_content = streamed.clone();

        // Drop invented citation numbers, then fact-check the answer against the sources.
        if grounding.injected {
            full_content = self
                .verify_answer(&grounding_cfg, &grounding, full_content, tx)
                .await?;
        }

        let tool_calls = self.parse_tool_calls(&full_content);
        let tool_results_with_markers = self.execute_tools(&tool_calls, &full_content).await;

        // PHASE 4: embed tool results into response.
        quench_log::info!("[STREAM_REFACTOR] Phase 4: Embedding results into response");
        let has_tool_results = !tool_results_with_markers.is_empty();
        let response_for_display = if has_tool_results {
            embed_tool_results_into_response(&full_content, tool_results_with_markers)
        } else {
            full_content.clone()
        };
        quench_log::info!("[STREAM_REFACTOR] Phase 4 complete: Response ready for database");

        let persisted = self
            .persist(&response_for_display, &grounding.web_sources)
            .await;

        // PHASE 6: the browser shows what was stored, then re-reads the conversation.
        quench_log::info!(
            "[STREAM_REFACTOR] Phase 6: Sending the end of the stream ({} tool results)",
            if has_tool_results { "some" } else { "no" }
        );
        let stored = response_for_display.trim();
        if stored != streamed.trim() {
            let _ = tx
                .send(event(&StreamEvent::Replace(stored.to_string())))
                .await;
        }
        let done = Done {
            conversation_id: self.req.conversation_id.clone(),
            user_message_id: (!self.req.skip_user_message)
                .then(|| persisted.ai_parent_id.clone())
                .flatten(),
            message_id: persisted.ai_msg_id,
            title: persisted.title,
        };
        let _ = tx.send(event(&StreamEvent::Done(done))).await;
        Ok(())
    }

    /// Plans, searches and reads the web for the question, forwarding progress to the browser, and
    /// adds the numbered sources to the system prompt.
    async fn gather_grounding(
        &mut self,
        grounding_cfg: &crate::grounding::GroundingConfig,
        tx: &Tx,
    ) -> Result<GroundingState, Aborted> {
        let mut state = GroundingState::default();
        let question = if self.req.skip_user_message {
            self.history
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.content.clone())
                .unwrap_or_default()
        } else {
            self.req.message.clone()
        };
        state.question.clone_from(&question);
        if question.trim().is_empty() {
            return Ok(state);
        }

        let provider_name = self
            .req
            .search_provider
            .as_deref()
            .unwrap_or(&self.config.default_search_provider);
        let today = quench_core::time::Time::now().format("%Y-%m-%d").clone();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let gathering = crate::grounding::gather(
            grounding_cfg,
            &self.switchboard,
            &self.vllm,
            &self.search_provider_registry,
            provider_name,
            &self.instance,
            &self.history,
            &question,
            &today,
            Some(&progress_tx),
        );
        tokio::pin!(gathering);
        // Forward pipeline progress to the browser while the search runs.
        let gathered = loop {
            tokio::select! {
                result = &mut gathering => break result,
                Some(progress) = progress_rx.recv() => {
                    if tx.send(stage_event(&progress)).await.is_err() {
                        return Err(Aborted);
                    }
                }
            }
        };
        // Stages reported just before the pipeline finished are still queued, and `select!` may
        // have picked the finished pipeline over them.
        while let Ok(progress) = progress_rx.try_recv() {
            if tx.send(stage_event(&progress)).await.is_err() {
                return Err(Aborted);
            }
        }
        if let Some(grounding) = gathered {
            self.system_message.content.push_str(&grounding.block);
            state.web_sources = grounding.sources;
            state.evidence = grounding.evidence;
            state.injected = true;
        }
        Ok(state)
    }

    /// The prompt: the system message, as much recent history as the budget allows, and the new
    /// user message.
    fn assemble_messages(&self) -> Vec<ChatMessage> {
        let skip_user = self.req.skip_user_message;
        // The grounding block counts against the prompt budget before history is selected.
        let system_tokens = estimate_tokens(&self.system_message);

        let mut selected_history = std::collections::VecDeque::new();
        let mut current_budget_used = system_tokens + if skip_user { 0 } else { self.user_tokens };

        for msg in self.history.iter().rev() {
            let msg_tokens = estimate_tokens(msg);
            if current_budget_used + msg_tokens <= self.limits.prompt_budget {
                current_budget_used += msg_tokens;
                selected_history.push_front(msg.clone());
            } else {
                break;
            }
        }

        let mut messages = vec![self.system_message.clone()];
        messages.extend(selected_history);
        if !skip_user {
            messages.push(self.user_message.clone());
        }

        // Stay within the vLLM instance's per-prompt image limit, preferring the newest images.
        crate::files::images::cap_images(
            &mut messages,
            crate::files::images::max_images_per_request(),
        );
        messages
    }

    /// PHASE 1: streams the response to the browser chunk by chunk; returns the whole of it.
    async fn stream_answer(
        &self,
        messages: Vec<ChatMessage>,
        tools_json: Option<Vec<quench_serde::Value>>,
        tx: &Tx,
    ) -> Result<String, Aborted> {
        let max_tokens = u32::try_from(self.limits.reserved_for_generation).unwrap_or(u32::MAX);
        let mut stream = match self
            .vllm
            .chat_stream_with_tools(
                &self.instance.host,
                self.instance.port,
                &self.instance.model,
                messages,
                Some(max_tokens),
                tools_json,
            )
            .await
        {
            Ok(s) => s,
            Err(err) => {
                quench_log::error!("Failed to start chat stream: {}", err);
                let _ = tx.send(error_event(&err)).await;
                return Err(Aborted);
            }
        };

        quench_log::info!("[STREAM_REFACTOR] Phase 1: Streaming response");
        let mut full_content = String::new();
        while let Some(res) = stream.next().await {
            match res {
                Ok(content) => {
                    full_content.push_str(&content);
                    if tx.send(event(&StreamEvent::Delta(content))).await.is_err() {
                        return Err(Aborted);
                    }
                }
                Err(err) => {
                    let _ = tx.send(error_event(&err)).await;
                    return Err(Aborted);
                }
            }
        }

        quench_log::info!(
            "[STREAM_REFACTOR] Phase 1 complete: {} chars collected",
            full_content.len()
        );
        Ok(full_content)
    }

    /// Fact-checks a grounded answer against its sources (showing a "checking" line meanwhile).
    async fn verify_answer(
        &self,
        grounding_cfg: &crate::grounding::GroundingConfig,
        grounding: &GroundingState,
        full_content: String,
        tx: &Tx,
    ) -> Result<String, Aborted> {
        if grounding_cfg.verify && !grounding.evidence.is_empty() {
            let checking = event(&StreamEvent::Status(Status {
                stage: "checking".to_string(),
                detail: None,
            }));
            if tx.send(checking).await.is_err() {
                return Err(Aborted);
            }
        }
        Ok(crate::grounding::finalize_answer(
            grounding_cfg,
            &self.switchboard,
            &self.vllm,
            &self.instance,
            &full_content,
            grounding.web_sources.len(),
            &grounding.evidence,
            &self.today,
            &grounding.question,
        )
        .await)
    }

    /// PHASE 2: the tool calls in the answer, none for a meta-question about the tools.
    fn parse_tool_calls(&self, full_content: &str) -> Vec<crate::tools::ToolCall> {
        quench_log::info!(
            "[STREAM_REFACTOR] Phase 2: Parsing tool calls from {} chars",
            full_content.len()
        );

        let has_toolcall_tags =
            full_content.contains("<toolcall>") || full_content.contains("<tool_call>");
        quench_log::debug!(
            "[PARSER] Response contains tool call tags: {}",
            has_toolcall_tags
        );

        let mut tool_calls = crate::tools::parser::parse_tool_calls(full_content);
        quench_log::info!("[STREAM_REFACTOR] Found {} tool calls", tool_calls.len());

        if tool_calls.is_empty() && has_toolcall_tags {
            quench_log::warn!(
                "[PARSER] Tool call tags found but failed to parse them. Response preview: {}",
                &full_content[..full_content.len().min(500)]
            );
        }

        // Meta-questions about tools shouldn't trigger the tools themselves.
        if is_meta_question(&self.req.message) && !tool_calls.is_empty() {
            quench_log::warn!(
                "[STREAM_REFACTOR] Suppressing {} tool calls for meta-question",
                tool_calls.len()
            );
            tool_calls.clear();
        }
        tool_calls
    }

    /// Runs the raw tool output back through vLLM for a clean summary.
    async fn summarize_tool_output(&self, query: &str, raw: &str) -> String {
        let parse_prompt = format!(
            "Results for: {query}\n\n{raw}\n\nProvide the information clearly and concisely. No headers or title needed—just the useful content."
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
        if let Ok(mut parse_stream) = self
            .vllm
            .chat_stream(
                &self.instance.host,
                self.instance.port,
                &self.instance.model,
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
        parsed
    }

    /// PHASE 3: executes every tool call; each result keyed by the `<tool_call>` block it answers.
    async fn execute_tools(
        &self,
        tool_calls: &[crate::tools::ToolCall],
        full_content: &str,
    ) -> Vec<(String, String)> {
        quench_log::info!(
            "[STREAM_REFACTOR] Phase 3: Executing {} tools",
            tool_calls.len()
        );
        let search_provider = self
            .req
            .search_provider
            .as_deref()
            .unwrap_or(&self.config.default_search_provider);

        // Map of marker string → formatted HTML result
        let mut tool_results_with_markers: Vec<(String, String)> = Vec::new();

        // Matches every tag variant the parser supports, mismatched/unclosed included.
        let tool_result_re = quench_regex::Regex::new(
            r"(?s)<(?:tool_call|toolcall)>\s*(\{.*?\})\s*</(?:tool_call|toolcall)>",
        )
        .ok();

        for tool_call in tool_calls {
            let query = tool_call
                .arguments
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            quench_log::info!("[STREAM_REFACTOR] Executing tool: {}", tool_call.name);

            let result = if tool_call.name == "web_search" {
                let executor = crate::tools::web_search::WebSearchExecutor::new(
                    self.search_provider_registry.clone(),
                )
                .with_default_provider(search_provider.to_string());
                executor.execute(tool_call).await
            } else {
                self.tool_registry.execute(tool_call).await
            };

            let formatted_result = if result.is_error {
                render_tool_result(tool_call, &result)
            } else {
                let mut summarized = result.clone();
                summarized.content = self.summarize_tool_output(query, &result.content).await;
                render_tool_result(tool_call, &summarized)
            };

            if let Some(re) = &tool_result_re
                && let Some(marker_text) = marker_for_call(tool_call, query, full_content, re)
            {
                quench_log::info!(
                    "[STREAM_REFACTOR] Registered result for marker: {}",
                    &marker_text[..marker_text.len().min(50)]
                );
                tool_results_with_markers.push((marker_text, formatted_result));
            }
        }

        quench_log::info!(
            "[STREAM_REFACTOR] Phase 3 complete: {} tool results collected",
            tool_results_with_markers.len()
        );
        tool_results_with_markers
    }

    /// Writes the conversation row (deriving a title for a new one), unless it already exists.
    async fn upsert_conversation(&self) -> crate::domain::models::Conversation {
        let conv_repo = self.db.repository::<crate::domain::models::Conversation>();
        // A blank title means the conversation was created lazily; derive one from the message.
        let title = match &self.stored.title {
            Some(t) if !t.trim().is_empty() => t.clone(),
            _ => preview(&self.req.message),
        };

        let conv = crate::domain::models::Conversation {
            id: self.req.conversation_id.clone(),
            title,
            active_message_id: self.stored.active_message_id.clone(),
            owner: self.username.clone(),
            // Falls back to the stored link so a message without project_id can't detach it.
            project_id: self
                .req
                .project_id
                .clone()
                .or_else(|| self.stored.project_id.clone()),
            updated_at: quench_core::time::Time::now().to_rfc3339(),
        };

        let exists = conv_repo
            .read(&self.req.conversation_id)
            .await
            .is_ok_and(|o| o.is_some());
        if exists {
            if let Err(err) = conv_repo.update(&conv).await {
                quench_log::error!("Failed to update conversation: {}", err);
            }
        } else if let Err(err) = conv_repo.create(&conv).await {
            quench_log::error!("Failed to create conversation: {}", err);
        }
        conv
    }

    /// Stores the user's message (unless this is a regeneration) and returns the id the answer
    /// hangs from.
    async fn store_user_message(&self) -> Option<String> {
        let req = &self.req;
        if req.skip_user_message {
            return req.parent_id.clone();
        }
        let msg_repo = self.db.repository::<crate::domain::models::Message>();
        let user_msg_id = quench_core::uuid::Uuid::new_v4().to_string();
        let user_msg = crate::domain::models::Message {
            id: user_msg_id.clone(),
            conversation_id: req.conversation_id.clone(),
            parent_id: self.stored.active_message_id.clone(),
            role: "user".to_string(),
            content: req.message.trim().to_string(),
            created_at: quench_core::time::Time::now().to_rfc3339(),
        };
        if let Err(err) = msg_repo.create(&user_msg).await {
            quench_log::error!("Failed to create user message: {}", err);
        }
        // Link any files staged in the composer to this user message.
        let staged_ids = req.file_id_list();
        if !staged_ids.is_empty()
            && let Err(err) = crate::routers::files::link_files_to_message(
                &self.db,
                &staged_ids,
                &user_msg_id,
                &req.conversation_id,
                &self.username,
            )
            .await
        {
            quench_log::error!("Failed to link attachments to message: {}", err);
        }
        Some(user_msg_id)
    }

    /// PHASE 5: stores the conversation, the user's message and the answer, and the sources the
    /// answer was built from.
    async fn persist(
        &self,
        response_for_display: &str,
        web_sources: &[crate::grounding::WebSource],
    ) -> Persisted {
        let mut conv = self.upsert_conversation().await;
        let ai_parent_id = self.store_user_message().await;

        // Stores the display copy, so embedded tool results persist too.
        let msg_repo = self.db.repository::<crate::domain::models::Message>();
        let ai_msg_id = quench_core::uuid::Uuid::new_v4().to_string();
        let ai_msg = crate::domain::models::Message {
            id: ai_msg_id.clone(),
            conversation_id: self.req.conversation_id.clone(),
            parent_id: ai_parent_id.clone(),
            role: "assistant".to_string(),
            content: response_for_display.trim().to_string(),
            created_at: quench_core::time::Time::now().to_rfc3339(),
        };
        if let Err(err) = msg_repo.create(&ai_msg).await {
            quench_log::error!("Failed to create AI message: {}", err);
        }

        // Record auto-injected RAG sources against this message for attribution.
        if !self.injected_rag_hits.is_empty()
            && let Err(err) = crate::files::rag::record_rag_contexts(
                &self.db,
                &ai_msg_id,
                &self.injected_rag_hits,
                "auto",
            )
            .await
        {
            quench_log::error!("Failed to record RAG sources: {}", err);
        }

        // Record the web pages the answer was grounded on, so they still show after a reload.
        if let Err(err) =
            crate::files::rag::record_web_sources(&self.db, &ai_msg_id, web_sources).await
        {
            quench_log::error!("Failed to record web sources: {}", err);
        }

        conv.active_message_id = Some(ai_msg_id.clone());
        conv.updated_at = quench_core::time::Time::now().to_rfc3339();
        let conv_repo = self.db.repository::<crate::domain::models::Conversation>();
        if let Err(err) = conv_repo.update(&conv).await {
            quench_log::error!("Failed to update conversation tip: {}", err);
        }

        Persisted {
            ai_msg_id,
            ai_parent_id,
            title: conv.title,
        }
    }
}

/// The chat-capable instance the request names, or the i18n code of why there is none.
async fn find_chat_instance(
    switchboard: &SwitchboardClient,
    req: &ChatRequest,
) -> Result<crate::clients::switchboard::VllmInstance, &'static str> {
    let instances = switchboard.get_vllm_instances().await.map_err(|err| {
        quench_log::error!("Failed to get vLLM instances: {}", err);
        "api_error_switchboard_unavailable"
    })?;

    let Some(instance) = instances.into_iter().find(|i| i.id == req.instance_id) else {
        return Err("api_error_instance_not_found");
    };

    if !instance.is_chat_capable() {
        quench_log::warn!(
            "Chat request routed to non-chat instance '{}' (task={:?}); embedding instances do not serve chat completions",
            instance.id,
            instance.task
        );
        return Err("api_error_embedding_model_chat");
    }
    Ok(instance)
}

/// A stream that says what went wrong and ends: the browser's `EventSource` cannot read an error status.
fn failure_stream(code: &str) -> Response {
    let body = quench_core::stream::iter([Ok::<_, std::io::Error>(error_event(&code))]);
    Response::streaming(StatusCode::OK, body).header("content-type", "text/event-stream")
}

/// Pre-create so `RAG`/`file_list`/`file_search` can resolve the conversation on the first message,
/// which Phase 5 would otherwise persist too late.
async fn precreate_conversation(db: &Db, req: &ChatRequest, username: &str) {
    let conv_repo = db.repository::<crate::domain::models::Conversation>();
    if matches!(conv_repo.read(&req.conversation_id).await, Ok(None)) {
        let conv = crate::domain::models::Conversation {
            id: req.conversation_id.clone(),
            // Blank until Phase 5 derives the title from the message.
            title: String::new(),
            active_message_id: None,
            owner: username.to_string(),
            project_id: req.project_id.clone(),
            updated_at: quench_core::time::Time::now().to_rfc3339(),
        };
        if let Err(e) = conv_repo.create(&conv).await {
            quench_log::error!(
                "Failed to pre-create conversation {}: {}",
                req.conversation_id,
                e
            );
        }
    }
}

/// Logs whether the system prompt advertises the tools.
fn log_system_prompt(system_message: &ChatMessage) {
    quench_log::info!(
        "System prompt length: {} chars, contains AVAILABLE TOOLS: {}",
        system_message.content.len(),
        system_message.content.contains("AVAILABLE TOOLS")
    );

    let has_web_search = system_message.content.contains("web_search");
    let has_tools_section = system_message.content.contains("AVAILABLE TOOLS");

    if has_tools_section && has_web_search {
        quench_log::info!("✓ System prompt includes web_search tool definition");
    } else if has_tools_section && !has_web_search {
        quench_log::warn!(
            "✗ System prompt has AVAILABLE TOOLS section but missing web_search definition"
        );
    } else {
        quench_log::warn!("✗ System prompt does NOT include AVAILABLE TOOLS section");
    }
}

/// The user's message, with any staged image uploads attached for vision models.
async fn build_user_message(db: &Db, req: &ChatRequest, username: &str) -> ChatMessage {
    let mut current_user_message = ChatMessage {
        role: "user".to_string(),
        content: req.message.clone(),
        tool_calls: None,
        images: None,
    };

    // Attach staged image uploads so vision models can see them; non-image attachments flow through RAG instead.
    if !req.skip_user_message {
        let staged_images =
            crate::files::images::load_staged_images(db, &req.file_id_list(), username).await;
        if !staged_images.is_empty() {
            current_user_message.images = Some(staged_images);
        }
    }

    quench_log::info!("User message: {}", current_user_message.content);
    current_user_message
}

/// What is already stored of the conversation.
async fn load_stored(db: &Db, req: &ChatRequest) -> StoredConversation {
    let repo = db.repository::<crate::domain::models::Conversation>();
    match repo.read(&req.conversation_id).await {
        Ok(Some(conv)) => StoredConversation {
            active_message_id: conv.active_message_id,
            title: Some(conv.title),
            project_id: conv.project_id,
        },
        _ => StoredConversation {
            active_message_id: None,
            title: None,
            project_id: None,
        },
    }
}

/// The conversation so far; a regeneration bases it on the parent instead of the active tip.
async fn load_history(db: &Db, req: &ChatRequest, stored: &StoredConversation) -> Vec<ChatMessage> {
    let history_base_id = if req.skip_user_message {
        req.parent_id.as_deref()
    } else {
        stored.active_message_id.as_deref()
    };

    match history_base_id {
        Some(amid) => get_conversation_messages(db, Some(amid))
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

/// The answer of a turn, as server-sent events (see [`sage_core::StreamEvent`]).
///
/// The turn runs in a task of its own and the stream carries what it sends; a turn that failed before it began says so on the
/// stream.
#[get("/api/v1/turns/{id}/stream")]
#[allow(clippy::too_many_arguments)]
pub async fn stream_message(
    username: Username,
    Path(id): Path<String>,
    Inject(chat_state): Inject<ChatState>,
    Inject(switchboard): Inject<SwitchboardClient>,
    Inject(vllm): Inject<VllmClient>,
    Inject(config): Inject<SageConfig>,
    Inject(db): Inject<Db>,
    Inject(search_provider_registry): Inject<crate::tools::SearchProviderRegistry>,
    Inject(metrics_collector): Inject<crate::observability::metrics::MetricsCollector>,
    Inject(rate_limiter): Inject<tokio::sync::Mutex<crate::runtime::rate_limiter::RateLimiter>>,
    Inject(cost_tracker): Inject<crate::observability::cost_tracking::CostTracker>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };

    let message_id = id;

    let Some(req) = chat_state.pending_messages.get(&message_id) else {
        return Response::new(StatusCode::NOT_FOUND);
    };

    // A turn is started by whoever owns its conversation; nobody else may run it by guessing the id.
    // (A first message names a conversation that does not exist yet, which nobody owns.)
    if let Err(response) = owned_conversation(&db, &req.conversation_id, &username).await {
        return response;
    }

    quench_log::info!(
        "Chat request: search_provider={:?}, instance_id={}",
        req.search_provider,
        req.instance_id
    );

    let active_profile = resolve_profile(&req, &config);

    // Shadows the global tool_registry param with this request-specific one.
    let tool_registry = Arc::new(build_tool_registry(
        &active_profile,
        &username,
        &req,
        &RegistryDeps {
            search_provider_registry: &search_provider_registry,
            db: &db,
            switchboard: &switchboard,
            vllm: &vllm,
            metrics_collector: &metrics_collector,
            rate_limiter: &rate_limiter,
            cost_tracker: &cost_tracker,
        },
    ));

    let instance = match find_chat_instance(&switchboard, &req).await {
        Ok(instance) => instance,
        Err(code) => return failure_stream(code),
    };

    let limits = PromptLimits::for_context(instance.max_model_len.unwrap_or(2048) as usize);

    let mut system_message = ChatMessage {
        role: "system".to_string(),
        content: config.system_prompt_with_date(),
        tool_calls: None,
        images: None,
    };

    precreate_conversation(&db, &req, &username).await;

    // Advertise uploaded files and inject relevant excerpts when available.
    let injected_rag_hits: Vec<crate::files::rag::ChunkHit> =
        crate::files::rag::augment_system_prompt(
            &db,
            &switchboard,
            &vllm,
            &req.conversation_id,
            &req.message,
        )
        .await
        .map_or_else(Vec::new, |(rag_augmentation, hits)| {
            system_message.content.push_str(&rag_augmentation);
            hits
        });
    log_system_prompt(&system_message);

    let user_message = build_user_message(&db, &req, &username).await;
    let user_tokens = estimate_tokens(&user_message);
    let stored = load_stored(&db, &req).await;
    let history = load_history(&db, &req, &stored).await;

    let turn = Turn {
        chat_state,
        message_id,
        req,
        username,
        db,
        vllm,
        switchboard,
        config,
        search_provider_registry,
        tool_registry,
        active_profile,
        instance,
        limits,
        system_message,
        user_message,
        user_tokens,
        history,
        injected_rag_hits,
        stored,
        today: quench_core::time::Time::now().format("%Y-%m-%d"),
    };

    // Channel-backed, not `async_stream::stream!`: DB work inside makes an
    // inline generator `!Sync`, but the receiver stream is `Sync` regardless.
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(16);
    tokio::spawn(turn.run(tx));

    let sse_stream = quench_core::stream::StreamExt::map(
        quench_http::stream::receiver_stream(rx),
        Ok::<_, std::io::Error>,
    );
    Response::streaming(StatusCode::OK, sse_stream).header("content-type", "text/event-stream")
}

fn json_response(status: StatusCode, value: &quench_serde::Value) -> Response {
    Response::json(status, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[get("/ui/chat/stats/{conversation_id}")]
pub async fn token_stats(
    username: Username,
    Path(conversation_id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(sage_config): Inject<SageConfig>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };
    match owned_conversation(&db, &conversation_id, &username).await {
        Ok(Some(_)) => {}
        Ok(None) => return Response::new(StatusCode::NOT_FOUND),
        Err(response) => return response,
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
                &quench_serde::json!({
                    "success": true,
                    "stats": usage.to_json(),
                    "display": usage.format_display(),
                    "warning": usage.warning_message(),
                }),
            )
        }
        Err(err) => json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &quench_serde::json!({
                "success": false,
                "error": err,
            }),
        ),
    }
}

#[must_use]
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[must_use]
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
        quench_html::markdown::to_html(&result.content)
    };

    // Don't capitalize tool name in header
    let header_text = if tool_name == "web_search" {
        format!("{} \"{}\"", tool_name, html_escape(query))
    } else {
        tool_name.clone()
    };

    let content_html = content_html
        .replace("<h1>", "<h3>")
        .replace("</h1>", "</h3>");

    format!(
        r#"<div class="{css_class}"><div class="tool-header"><span class="tool-icon">{icon}</span><span class="tool-name">{header_text}</span></div><div class="tool-content">{content_html}</div></div>"#
    )
}

fn turn_started(id: String) -> Response {
    Response::json(StatusCode::OK, &TurnStarted { id })
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// The conversation, if it is stored and the person's: `Err` is the answer to give.
pub(crate) async fn owned_conversation(
    db: &Db,
    id: &str,
    username: &str,
) -> Result<Option<crate::domain::models::Conversation>, Response> {
    match db
        .repository::<crate::domain::models::Conversation>()
        .read(id)
        .await
    {
        Ok(Some(conv)) if conv.owner == username => Ok(Some(conv)),
        Ok(Some(_)) => Err(Response::new(StatusCode::FORBIDDEN)),
        Ok(None) => Ok(None),
        Err(err) => {
            quench_log::error!("Internal error: {}", err);
            Err(Response::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_internal",
            ))
        }
    }
}

/// Starts a turn: stores the request and answers with the id of the stream that carries the answer.
#[post("/api/v1/turns")]
pub async fn send_message(
    username: Username,
    Json(mut chat_req): Json<ChatRequest>,
    Inject(state): Inject<ChatState>,
    Inject(db): Inject<Db>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };
    chat_req.message = chat_req.message.trim().to_string();
    if chat_req.message.is_empty() {
        return Response::text(StatusCode::BAD_REQUEST, "api_error_empty_message");
    }
    // The conversation may be new; one that exists has to be the person's.
    if let Err(response) = owned_conversation(&db, &chat_req.conversation_id, &username).await {
        return response;
    }
    // The branch point of a new message is the conversation's tip; a client cannot choose another's.
    chat_req.skip_user_message = false;
    chat_req.parent_id = None;

    let message_id = Uuid::new_v4().to_string();
    state.pending_messages.insert(message_id.clone(), chat_req);
    turn_started(message_id)
}

#[derive(Debug, quench_serde::Deserialize)]
pub struct RegenerateRequest {
    pub message_id: String,
}

/// The first model that can chat.
async fn first_chat_instance(
    switchboard: &SwitchboardClient,
) -> Result<crate::clients::switchboard::VllmInstance, Response> {
    let instances = switchboard.get_vllm_instances().await.unwrap_or_default();
    instances
        .into_iter()
        .find(crate::clients::switchboard::VllmInstance::is_chat_capable)
        .ok_or_else(|| {
            Response::text(
                StatusCode::SERVICE_UNAVAILABLE,
                "api_error_no_models_available",
            )
        })
}

/// A new answer to the message before `message_id` (an answer of Sage's), next to the old one.
#[post("/api/v1/turns/regenerate")]
pub async fn regenerate(
    username: Username,
    Json(body): Json<RegenerateRequest>,
    Inject(state): Inject<ChatState>,
    Inject(db): Inject<Db>,
    Inject(switchboard): Inject<SwitchboardClient>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };
    let repo = db.repository::<crate::domain::models::Message>();
    let Ok(Some(msg)) = repo.read(&body.message_id).await else {
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

    let project_id = match owned_conversation(&db, &msg.conversation_id, &username).await {
        Ok(conv) => conv.and_then(|c| c.project_id),
        Err(response) => return response,
    };

    let Ok(Some(parent_msg)) = repo.read(&parent_id).await else {
        return Response::text(StatusCode::NOT_FOUND, "api_error_parent_not_found");
    };

    let instance = match first_chat_instance(&switchboard).await {
        Ok(instance) => instance,
        Err(response) => return response,
    };

    let message_id = Uuid::new_v4().to_string();
    state.pending_messages.insert(
        message_id.clone(),
        ChatRequest {
            instance_id: instance.id,
            message: parent_msg.content,
            conversation_id: msg.conversation_id,
            project_id,
            parent_id: Some(parent_id),
            skip_user_message: true,
            search_provider: None,
            capability_profile: None,
            tool_confirmations: Vec::new(),
            file_ids: String::new(),
        },
    );
    turn_started(message_id)
}

/// Opening a conversation whose last message is the person's (after an edit, or a turn that did not finish) has Sage answer it.
/// The answer is `{"id": null}` when the last message is Sage's.
#[post("/api/v1/conversations/{id}/resume")]
pub async fn resume(
    username: Username,
    Path(conversation_id): Path<String>,
    Inject(state): Inject<ChatState>,
    Inject(db): Inject<Db>,
    Inject(switchboard): Inject<SwitchboardClient>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };
    let conv = match owned_conversation(&db, &conversation_id, &username).await {
        Ok(Some(conv)) => conv,
        Ok(None) => {
            return Response::text(StatusCode::NOT_FOUND, "api_error_conversation_not_found");
        }
        Err(response) => return response,
    };
    let none = || {
        Response::json(StatusCode::OK, &sage_core::Resumed { id: None })
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
    };
    let Some(tip) = conv.active_message_id.clone() else {
        return none();
    };
    let Ok(Some(last)) = db
        .repository::<crate::domain::models::Message>()
        .read(&tip)
        .await
    else {
        return none();
    };
    if last.role != "user" {
        return none();
    }
    let Ok(instance) = first_chat_instance(&switchboard).await else {
        return none();
    };

    let id = Uuid::new_v4().to_string();
    state.pending_messages.insert(
        id.clone(),
        ChatRequest {
            instance_id: instance.id,
            message: last.content,
            conversation_id: last.conversation_id,
            project_id: conv.project_id,
            parent_id: Some(last.id),
            skip_user_message: true,
            search_provider: None,
            capability_profile: None,
            tool_confirmations: Vec::new(),
            file_ids: String::new(),
        },
    );
    Response::json(StatusCode::OK, &sage_core::Resumed { id: Some(id) })
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = send_message as fn(_, _, _, _) -> _;
    let _ = stream_message as fn(_, _, _, _, _, _, _, _, _, _, _) -> _;
    let _ = regenerate as fn(_, _, _, _, _) -> _;
    let _ = resume as fn(_, _, _, _, _) -> _;
    let _ = token_stats as fn(_, _, _, _) -> _;
}
