use quench_serde::{Deserialize, Serialize};
use std::fmt;

pub mod calculator;
pub mod capabilities;
pub mod code_executor;
pub mod command;
pub mod file_list;
pub mod file_ops;
pub mod file_search;
pub mod parser;
pub mod search_providers;
pub mod web_fetch;
pub mod web_search;

pub use capabilities::{CapabilityProfile, Tool};

pub use search_providers::{SearchProvider, SearchProviderRegistry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub parameters: ToolParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameters {
    #[serde(rename = "type")]
    pub param_type: String,
    pub properties: quench_serde::Value,
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: quench_serde::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: String,
    pub is_error: bool,
}

impl fmt::Display for ToolResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.content)
    }
}

#[must_use]
pub fn get_all_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        web_search::get_definition(),
        calculator::get_definition(),
        web_fetch::get_definition(),
        file_ops::get_definition(),
        file_search::get_definition(),
        file_list::get_definition(),
        command::get_definition(),
        code_executor::get_definition(),
    ]
}

#[must_use]
pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    get_all_tool_definitions()
}

#[must_use]
pub fn get_tool_definitions_filtered(profile: &CapabilityProfile) -> Vec<ToolDefinition> {
    get_all_tool_definitions()
        .into_iter()
        .filter(|def| profile.enabled_tool_names().contains(&def.name.as_str()))
        .collect()
}

#[must_use]
pub fn get_tool_definitions_for_prompt() -> String {
    let tools = get_tool_definitions();
    quench_serde::json::to_string_pretty(&tools)
}

#[must_use]
pub fn get_tool_definitions_for_prompt_filtered(profile: &CapabilityProfile) -> String {
    let tools = get_tool_definitions_filtered(profile);
    quench_serde::json::to_string_pretty(&tools)
}

#[quench_core::async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult;
}

/// An error result for `tool_call`, which the model sees in place of the tool's output.
fn error_result(tool_call: &ToolCall, content: String) -> ToolResult {
    ToolResult {
        tool_use_id: tool_call.id.clone(),
        content,
        is_error: true,
    }
}

pub struct ToolRegistry {
    executors: std::collections::HashMap<String, Box<dyn ToolExecutor>>,
    profile: Option<CapabilityProfile>,
    user_id: Option<String>,
    conversation_id: Option<String>,
    confirmations: std::collections::HashSet<String>,
    metrics_collector: Option<std::sync::Arc<crate::observability::metrics::MetricsCollector>>,
    rate_limiter:
        Option<std::sync::Arc<tokio::sync::Mutex<crate::runtime::rate_limiter::RateLimiter>>>,
    cost_tracker: Option<std::sync::Arc<crate::observability::cost_tracking::CostTracker>>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            executors: std::collections::HashMap::new(),
            profile: None,
            user_id: None,
            conversation_id: None,
            confirmations: std::collections::HashSet::new(),
            metrics_collector: None,
            rate_limiter: None,
            cost_tracker: None,
        }
    }

    #[must_use]
    pub fn with_profile(profile: CapabilityProfile) -> Self {
        Self {
            executors: std::collections::HashMap::new(),
            profile: Some(profile),
            user_id: None,
            conversation_id: None,
            confirmations: std::collections::HashSet::new(),
            metrics_collector: None,
            rate_limiter: None,
            cost_tracker: None,
        }
    }

    #[must_use]
    pub fn with_context(
        profile: CapabilityProfile,
        user_id: Option<String>,
        conversation_id: Option<String>,
    ) -> Self {
        Self {
            executors: std::collections::HashMap::new(),
            profile: Some(profile),
            user_id,
            conversation_id,
            confirmations: std::collections::HashSet::new(),
            metrics_collector: None,
            rate_limiter: None,
            cost_tracker: None,
        }
    }

    pub fn set_metrics_collector(
        &mut self,
        collector: std::sync::Arc<crate::observability::metrics::MetricsCollector>,
    ) {
        self.metrics_collector = Some(collector);
    }

    pub fn set_rate_limiter(
        &mut self,
        limiter: std::sync::Arc<tokio::sync::Mutex<crate::runtime::rate_limiter::RateLimiter>>,
    ) {
        self.rate_limiter = Some(limiter);
    }

    pub fn set_cost_tracker(
        &mut self,
        tracker: std::sync::Arc<crate::observability::cost_tracking::CostTracker>,
    ) {
        self.cost_tracker = Some(tracker);
    }

    pub fn register(&mut self, name: String, executor: Box<dyn ToolExecutor>) {
        self.executors.insert(name, executor);
    }

    /// Refuses a call the rate limiter or the capability profile does not allow: the error result to
    /// return in its place, or `None` when the call may run.
    async fn refuse(&self, tool_call: &ToolCall, profile_name: &str) -> Option<ToolResult> {
        if let Some(limiter) = &self.rate_limiter {
            let limiter_guard = limiter.lock().await;
            if let Err(rate_limit_err) = limiter_guard.check_rate_limit(
                profile_name,
                self.user_id.as_deref(),
                self.conversation_id.as_deref(),
            ) {
                quench_log::warn!(
                    target: "audit",
                    user_id = ?self.user_id,
                    conversation_id = ?self.conversation_id,
                    error = %rate_limit_err,
                    "Rate limit exceeded"
                );

                return Some(error_result(
                    tool_call,
                    format!("Rate limit exceeded: {rate_limit_err}"),
                ));
            }
        }

        let profile = self.profile.as_ref()?;
        if !profile
            .enabled_tool_names()
            .contains(&tool_call.name.as_str())
        {
            crate::observability::audit::AuditLogger::log_restricted_tool_attempt(
                &tool_call.name,
                profile_name,
                self.user_id.as_deref(),
                self.conversation_id.as_deref(),
            );

            return Some(error_result(
                tool_call,
                format!(
                    "Tool '{}' is not available in the '{}' capability profile",
                    tool_call.name, profile.name
                ),
            ));
        }

        // Check if this tool requires confirmation
        if profile.requires_confirmation(&tool_call.name) && !self.has_confirmation(&tool_call.name)
        {
            crate::observability::audit::AuditLogger::log_confirmation_required(
                &tool_call.name,
                profile_name,
                self.user_id.as_deref(),
                self.conversation_id.as_deref(),
            );

            return Some(error_result(
                tool_call,
                format!(
                    "Tool '{}' requires explicit confirmation before execution. Please confirm with 'confirm_{}': true in your request.",
                    tool_call.name, tool_call.name
                ),
            ));
        }
        None
    }

    /// Runs the tool once, bounded by `timeout_secs`; the result, and whether it timed out.
    async fn run_once(&self, tool_call: &ToolCall, timeout_secs: u64) -> (ToolResult, bool) {
        let Some(executor) = self.executors.get(&tool_call.name) else {
            return (
                error_result(tool_call, format!("Tool '{}' not found", tool_call.name)),
                false,
            );
        };
        tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout_secs),
            executor.execute(tool_call),
        )
        .await
        .map_or_else(
            |_| {
                (
                    error_result(
                        tool_call,
                        format!(
                            "Tool '{}' execution timed out after {} seconds",
                            tool_call.name, timeout_secs
                        ),
                    ),
                    true,
                )
            },
            |executed| (executed, false),
        )
    }

    pub async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        let start_time = std::time::Instant::now();
        let profile_name = self.profile.as_ref().map_or("default", |p| p.name.as_str());

        if let Some(refusal) = self.refuse(tool_call, profile_name).await {
            return refusal;
        }

        let timeout_secs = self
            .profile
            .as_ref()
            .map_or(60, |p| p.get_timeout_for_tool(&tool_call.name));

        let retry_policy = crate::runtime::retry_policy::get_retry_policy(&tool_call.name);

        let mut timeout_occurred = false;
        let mut retry_count = 0;

        // Retry loop with exponential backoff
        let result = loop {
            let (result, timed_out) = self.run_once(tool_call, timeout_secs).await;
            timeout_occurred |= timed_out;

            // Check if we should retry
            if result.is_error && retry_policy.should_retry(retry_count) {
                // Transient errors that can be retried
                let is_transient = result.content.contains("timeout")
                    || result.content.contains("connection")
                    || result.content.contains("temporarily")
                    || result.content.contains("unavailable");

                if is_transient {
                    let backoff = retry_policy.calculate_backoff(retry_count);
                    quench_log::info!(
                        tool = %tool_call.name,
                        attempt = retry_count + 1,
                        backoff_ms = backoff.as_millis(),
                        "Retrying tool execution after transient error"
                    );
                    tokio::time::sleep(backoff).await;
                    retry_count += 1;
                    continue;
                }
            }

            break result;
        };

        let elapsed = start_time.elapsed().as_millis();

        // Record metrics
        if let Some(collector) = &self.metrics_collector {
            collector.record_tool_execution(
                profile_name,
                &tool_call.name,
                elapsed,
                !result.is_error,
                timeout_occurred,
            );
        }

        crate::observability::audit::AuditLogger::log_tool_execution(
            &tool_call.name,
            profile_name,
            self.user_id.clone(),
            self.conversation_id.clone(),
            !result.is_error,
            if result.is_error {
                Some(result.content.clone())
            } else {
                None
            },
            elapsed,
        );

        result
    }

    #[must_use]
    pub fn get_definitions(&self) -> Vec<ToolDefinition> {
        self.profile
            .as_ref()
            .map_or_else(get_tool_definitions, get_tool_definitions_filtered)
    }

    #[must_use]
    pub const fn profile(&self) -> Option<&CapabilityProfile> {
        self.profile.as_ref()
    }

    pub fn set_context(&mut self, user_id: Option<String>, conversation_id: Option<String>) {
        self.user_id = user_id;
        self.conversation_id = conversation_id;
    }

    pub fn add_confirmations(&mut self, tools: &[&str]) {
        for tool in tools {
            self.confirmations.insert(tool.to_string());
        }
    }

    #[must_use]
    pub fn has_confirmation(&self, tool_name: &str) -> bool {
        self.confirmations.contains(tool_name)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ToolRegistry { .. }")
    }
}
