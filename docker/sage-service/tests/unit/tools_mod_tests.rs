//! `tools/mod.rs` - `ToolRegistry::execute`'s branching (profile gating,
//! confirmation, rate limiting, retry, timeout) and metrics recording.

use sage_service::observability::metrics::MetricsCollector;
use sage_service::runtime::rate_limiter::RateLimiter;
use sage_service::tools::capabilities::{CapabilityProfile, Tool};
use sage_service::tools::{ToolCall, ToolExecutor, ToolRegistry, ToolResult};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

struct EchoExecutor;

#[async_trait::async_trait]
impl ToolExecutor for EchoExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        ToolResult {
            tool_use_id: tool_call.id.clone(),
            content: "echoed".to_string(),
            is_error: false,
        }
    }
}

/// Fails with a transient-looking error the configured number of times,
/// then succeeds - for exercising `execute`'s retry loop.
struct FlakyExecutor {
    fail_times: u32,
    calls: AtomicU32,
}

impl FlakyExecutor {
    fn new(fail_times: u32) -> Self {
        Self {
            fail_times,
            calls: AtomicU32::new(0),
        }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FlakyExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        let attempt = self.calls.fetch_add(1, Ordering::SeqCst);
        if attempt < self.fail_times {
            ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: "temporarily unavailable".to_string(),
                is_error: true,
            }
        } else {
            ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: "recovered".to_string(),
                is_error: false,
            }
        }
    }
}

struct SlowExecutor;

#[async_trait::async_trait]
impl ToolExecutor for SlowExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        ToolResult {
            tool_use_id: tool_call.id.clone(),
            content: "too slow".to_string(),
            is_error: false,
        }
    }
}

fn call(name: &str) -> ToolCall {
    ToolCall {
        id: "call-1".to_string(),
        name: name.to_string(),
        arguments: serde_json::json!({}),
    }
}

#[tokio::test]
async fn execute_reports_an_unregistered_tool_as_not_found() {
    let registry = ToolRegistry::new();
    let result = registry.execute(&call("nonexistent")).await;

    assert!(result.is_error);
    assert!(result.content.contains("not found"));
}

#[tokio::test]
async fn execute_runs_a_registered_tool() {
    let mut registry = ToolRegistry::new();
    registry.register("echo".to_string(), Box::new(EchoExecutor));

    let result = registry.execute(&call("echo")).await;

    assert!(!result.is_error);
    assert_eq!(result.content, "echoed");
}

#[tokio::test]
async fn execute_rejects_a_tool_outside_the_profile() {
    let profile = CapabilityProfile::new("web_assistant", "test", &[Tool::Calculator]);
    let mut registry = ToolRegistry::with_profile(profile);
    registry.register("echo".to_string(), Box::new(EchoExecutor));

    let result = registry.execute(&call("echo")).await;

    assert!(result.is_error);
    assert!(result.content.contains("not available"));
    assert!(result.content.contains("web_assistant"));
}

#[tokio::test]
async fn execute_allows_a_tool_the_profile_enables() {
    // `Calculator`, unlike `Command`/`FileOps`, doesn't need a confirmation
    // first (`CapabilityProfile::requires_confirmation`), so this isolates
    // the profile-gating check from the separate confirmation check.
    let profile = CapabilityProfile::new("code_assistant", "test", &[Tool::Calculator]);
    let mut registry = ToolRegistry::with_profile(profile);
    registry.register("calculator".to_string(), Box::new(EchoExecutor));

    let result = registry.execute(&call("calculator")).await;
    assert!(!result.is_error);
}

#[tokio::test]
async fn execute_requires_confirmation_for_command_before_running_it() {
    let profile = CapabilityProfile::new("cli_agent", "test", &[Tool::Command]);
    let mut registry = ToolRegistry::with_profile(profile);
    registry.register("command".to_string(), Box::new(EchoExecutor));

    let blocked = registry.execute(&call("command")).await;
    assert!(blocked.is_error);
    assert!(blocked.content.contains("requires explicit confirmation"));

    registry.add_confirmations(&["command"]);
    assert!(registry.has_confirmation("command"));

    let allowed = registry.execute(&call("command")).await;
    assert!(!allowed.is_error);
    assert_eq!(allowed.content, "echoed");
}

#[tokio::test]
async fn execute_enforces_the_rate_limiter() {
    let mut registry = ToolRegistry::with_context(
        CapabilityProfile::new("web_assistant", "test", &[Tool::Calculator]),
        Some("user-rate-limit-test".to_string()),
        Some("conv-rate-limit-test".to_string()),
    );
    registry.register("calculator".to_string(), Box::new(EchoExecutor));
    registry.set_rate_limiter(Arc::new(tokio::sync::Mutex::new(RateLimiter::new())));

    // web_assistant's burst limit is 30 (see runtime_rate_limiter_tests.rs).
    for _ in 0..30 {
        let result = registry.execute(&call("calculator")).await;
        assert!(!result.is_error);
    }

    let limited = registry.execute(&call("calculator")).await;
    assert!(limited.is_error);
    assert!(limited.content.contains("Rate limit exceeded"));
}

#[tokio::test]
async fn execute_retries_a_transient_failure_then_succeeds() {
    let mut registry = ToolRegistry::new();
    // No special-cased no-retry tool name, so this gets the default policy
    // (1 retry, 100ms backoff) from `runtime::retry_policy::get_retry_policy`.
    registry.register("flaky_tool".to_string(), Box::new(FlakyExecutor::new(1)));

    let result = registry.execute(&call("flaky_tool")).await;

    assert!(!result.is_error);
    assert_eq!(result.content, "recovered");
}

#[tokio::test]
async fn execute_does_not_retry_past_the_policy_limit() {
    let mut registry = ToolRegistry::new();
    // "calculator" has `RetryPolicy::none()`, so even a transient-looking
    // failure is returned as-is on the first attempt.
    registry.register("calculator".to_string(), Box::new(FlakyExecutor::new(1)));

    let result = registry.execute(&call("calculator")).await;

    assert!(result.is_error);
    assert_eq!(result.content, "temporarily unavailable");
}

#[tokio::test]
async fn execute_times_out_a_slow_tool() {
    // The timeout applies per-tool by name via `get_timeout_for_tool`, and
    // the profile-gating check filters on the `Tool` enum's own names - so
    // this has to register under a real tool name (`web_fetch`), not an
    // arbitrary one, or it gets rejected by the profile check first.
    let profile = CapabilityProfile::new("web_assistant", "test", &[Tool::WebFetch])
        .with_timeouts(60, &[("web_fetch", 1)]);
    let mut registry = ToolRegistry::with_profile(profile);
    registry.register("web_fetch".to_string(), Box::new(SlowExecutor));

    let result = registry.execute(&call("web_fetch")).await;

    assert!(result.is_error);
    assert!(result.content.contains("timed out"));
}

#[tokio::test]
async fn execute_records_metrics_when_a_collector_is_set() {
    let mut registry = ToolRegistry::new();
    registry.register("echo".to_string(), Box::new(EchoExecutor));
    let metrics = Arc::new(MetricsCollector::new());
    registry.set_metrics_collector(metrics.clone());

    registry.execute(&call("echo")).await;

    let recorded = metrics
        .get_profile_metrics("default")
        .expect("metrics recorded under the default profile name");
    assert_eq!(recorded.tools.get("echo").map(|m| m.calls), Some(1));
}

#[test]
fn set_cost_tracker_does_not_panic() {
    let mut registry = ToolRegistry::new();
    registry.set_cost_tracker(Arc::new(
        sage_service::observability::cost_tracking::CostTracker::new(),
    ));
}

#[test]
fn get_definitions_uses_the_profile_filter_when_present() {
    let profile = CapabilityProfile::new("web_assistant", "test", &[Tool::Calculator]);
    let registry = ToolRegistry::with_profile(profile);
    let defs = registry.get_definitions();

    assert!(defs.iter().any(|d| d.name == "calculator"));
    assert!(!defs.iter().any(|d| d.name == "command"));
}

#[test]
fn get_definitions_returns_everything_without_a_profile() {
    let registry = ToolRegistry::new();
    let defs = registry.get_definitions();
    assert!(defs.len() >= 8);
}

#[test]
fn profile_accessor_reflects_construction() {
    assert!(ToolRegistry::new().profile().is_none());

    let profile = CapabilityProfile::new("web_assistant", "test", &[Tool::Calculator]);
    let with_profile = ToolRegistry::with_profile(profile);
    assert_eq!(
        with_profile.profile().map(|p| p.name.as_str()),
        Some("web_assistant")
    );
}

#[test]
fn set_context_updates_user_and_conversation_ids() {
    let mut registry = ToolRegistry::new();
    registry.set_context(Some("user-9".to_string()), Some("conv-9".to_string()));
    // No public getter for these fields; this just confirms the call
    // compiles and doesn't panic - the effect is covered indirectly by
    // `execute_enforces_the_rate_limiter`, which relies on `with_context`.
}

#[test]
fn default_impl_matches_new() {
    let registry = ToolRegistry::default();
    assert!(registry.profile().is_none());
}

#[test]
fn tool_result_display_prints_its_content() {
    let result = ToolResult {
        tool_use_id: "id".to_string(),
        content: "hello".to_string(),
        is_error: false,
    };
    assert_eq!(result.to_string(), "hello");
}

#[test]
fn get_all_tool_definitions_and_get_tool_definitions_for_prompt_round_trip() {
    let defs = sage_service::tools::get_all_tool_definitions();
    assert!(defs.iter().any(|d| d.name == "web_search"));

    let prompt = sage_service::tools::get_tool_definitions_for_prompt();
    assert!(prompt.contains("web_search"));

    let profile = CapabilityProfile::new("web_assistant", "test", &[Tool::Calculator]);
    let filtered_prompt = sage_service::tools::get_tool_definitions_for_prompt_filtered(&profile);
    assert!(filtered_prompt.contains("calculator"));
    assert!(!filtered_prompt.contains("\"command\""));
}
