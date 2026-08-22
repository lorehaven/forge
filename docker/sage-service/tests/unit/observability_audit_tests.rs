//! `observability/audit.rs`. `AuditLogger`'s methods only emit `tracing`
//! events - there's no return value or observable state to assert on, so
//! these are "doesn't panic under every branch" tests, per the project's
//! convention for pure logging/presentation glue.

use sage_service::observability::audit::AuditLogger;

#[test]
fn log_tool_execution_success_does_not_panic() {
    AuditLogger::log_tool_execution(
        "calculator",
        "web_assistant",
        Some("user-1".to_string()),
        Some("conv-1".to_string()),
        true,
        None,
        42,
    );
}

#[test]
fn log_tool_execution_failure_does_not_panic() {
    AuditLogger::log_tool_execution(
        "web_search",
        "code_assistant",
        None,
        None,
        false,
        Some("timed out".to_string()),
        1000,
    );
}

#[test]
fn log_restricted_tool_attempt_does_not_panic() {
    AuditLogger::log_restricted_tool_attempt(
        "code_executor",
        "web_assistant",
        Some("user-2".to_string()),
        None,
    );
}

#[test]
fn log_confirmation_required_does_not_panic() {
    AuditLogger::log_confirmation_required(
        "command",
        "cli_agent",
        None,
        Some("conv-2".to_string()),
    );
}
