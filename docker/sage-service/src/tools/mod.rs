use serde::{Deserialize, Serialize};
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
    pub properties: serde_json::Value,
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
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

pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    get_all_tool_definitions()
}

pub fn get_tool_definitions_filtered(profile: &CapabilityProfile) -> Vec<ToolDefinition> {
    get_all_tool_definitions()
        .into_iter()
        .filter(|def| profile.enabled_tool_names().contains(&def.name.as_str()))
        .collect()
}

pub fn get_tool_definitions_for_prompt() -> String {
    let tools = get_tool_definitions();
    match serde_json::to_string_pretty(&tools) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to serialize tool definitions: {}", e);
            String::new()
        }
    }
}

pub fn get_tool_definitions_for_prompt_filtered(profile: &CapabilityProfile) -> String {
    let tools = get_tool_definitions_filtered(profile);
    match serde_json::to_string_pretty(&tools) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to serialize tool definitions: {}", e);
            String::new()
        }
    }
}

#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult;
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

    pub async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        let start_time = std::time::Instant::now();
        let profile_name = self
            .profile
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("default");

        // Check rate limiting
        if let Some(limiter) = &self.rate_limiter {
            let limiter_guard = limiter.lock().await;
            if let Err(rate_limit_err) = limiter_guard.check_rate_limit(
                profile_name,
                self.user_id.as_deref(),
                self.conversation_id.as_deref(),
            ) {
                tracing::warn!(
                    target: "audit",
                    user_id = ?self.user_id,
                    conversation_id = ?self.conversation_id,
                    error = %rate_limit_err,
                    "Rate limit exceeded"
                );

                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: format!("Rate limit exceeded: {}", rate_limit_err),
                    is_error: true,
                };
            }
        }

        if let Some(profile) = &self.profile {
            if !profile
                .enabled_tool_names()
                .contains(&tool_call.name.as_str())
            {
                crate::observability::audit::AuditLogger::log_restricted_tool_attempt(
                    &tool_call.name,
                    profile_name,
                    self.user_id.clone(),
                    self.conversation_id.clone(),
                );

                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: format!(
                        "Tool '{}' is not available in the '{}' capability profile",
                        tool_call.name, profile.name
                    ),
                    is_error: true,
                };
            }

            // Check if this tool requires confirmation
            if profile.requires_confirmation(&tool_call.name)
                && !self.has_confirmation(&tool_call.name)
            {
                crate::observability::audit::AuditLogger::log_confirmation_required(
                    &tool_call.name,
                    profile_name,
                    self.user_id.clone(),
                    self.conversation_id.clone(),
                );

                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: format!(
                        "Tool '{}' requires explicit confirmation before execution. Please confirm with 'confirm_{}': true in your request.",
                        tool_call.name, tool_call.name
                    ),
                    is_error: true,
                };
            }
        }

        let timeout_secs = self
            .profile
            .as_ref()
            .map(|p| p.get_timeout_for_tool(&tool_call.name))
            .unwrap_or(60);

        let retry_policy = crate::runtime::retry_policy::get_retry_policy(&tool_call.name);

        let mut timeout_occurred = false;
        let mut retry_count = 0;
        let mut result: Option<ToolResult>;

        // Retry loop with exponential backoff
        loop {
            result = match self.executors.get(&tool_call.name) {
                Some(executor) => {
                    match tokio::time::timeout(
                        tokio::time::Duration::from_secs(timeout_secs),
                        executor.execute(tool_call),
                    )
                    .await
                    {
                        Ok(exec_result) => Some(exec_result),
                        Err(_) => {
                            timeout_occurred = true;
                            Some(ToolResult {
                                tool_use_id: tool_call.id.clone(),
                                content: format!(
                                    "Tool '{}' execution timed out after {} seconds",
                                    tool_call.name, timeout_secs
                                ),
                                is_error: true,
                            })
                        }
                    }
                }
                None => Some(ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: format!("Tool '{}' not found", tool_call.name),
                    is_error: true,
                }),
            };

            // Check if we should retry
            if let Some(ref res) = result
                && res.is_error
                && retry_policy.should_retry(retry_count)
            {
                // Transient errors that can be retried
                let is_transient = res.content.contains("timeout")
                    || res.content.contains("connection")
                    || res.content.contains("temporarily")
                    || res.content.contains("unavailable");

                if is_transient {
                    let backoff = retry_policy.calculate_backoff(retry_count);
                    tracing::info!(
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

            break;
        }

        let result = result.unwrap_or_else(|| ToolResult {
            tool_use_id: tool_call.id.clone(),
            content: "Unexpected error during execution".to_string(),
            is_error: true,
        });

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

    pub fn get_definitions(&self) -> Vec<ToolDefinition> {
        if let Some(profile) = &self.profile {
            get_tool_definitions_filtered(profile)
        } else {
            get_tool_definitions()
        }
    }

    pub fn profile(&self) -> Option<&CapabilityProfile> {
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

    pub fn has_confirmation(&self, tool_name: &str) -> bool {
        self.confirmations.contains(tool_name)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
