use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub timestamp: String,
    pub tool_name: String,
    pub capability_profile: String,
    pub user_id: Option<String>,
    pub conversation_id: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
    pub execution_ms: u128,
}

pub struct AuditLogger;

impl AuditLogger {
    pub fn log_tool_execution(
        tool_name: &str,
        profile_name: &str,
        user_id: Option<String>,
        conversation_id: Option<String>,
        success: bool,
        error_message: Option<String>,
        execution_ms: u128,
    ) {
        let entry = AuditLogEntry {
            timestamp: Utc::now().to_rfc3339(),
            tool_name: tool_name.to_string(),
            capability_profile: profile_name.to_string(),
            user_id,
            conversation_id,
            success,
            error_message,
            execution_ms,
        };

        if success {
            tracing::info!(
                target: "audit",
                tool = %entry.tool_name,
                profile = %entry.capability_profile,
                user_id = ?entry.user_id,
                conversation_id = ?entry.conversation_id,
                duration_ms = entry.execution_ms,
                "Tool executed successfully"
            );
        } else {
            tracing::warn!(
                target: "audit",
                tool = %entry.tool_name,
                profile = %entry.capability_profile,
                user_id = ?entry.user_id,
                conversation_id = ?entry.conversation_id,
                error = ?entry.error_message,
                duration_ms = entry.execution_ms,
                "Tool execution failed"
            );
        }
    }

    pub fn log_restricted_tool_attempt(
        tool_name: &str,
        profile_name: &str,
        user_id: Option<String>,
        conversation_id: Option<String>,
    ) {
        tracing::warn!(
            target: "audit",
            tool = %tool_name,
            profile = %profile_name,
            user_id = ?user_id,
            conversation_id = ?conversation_id,
            "Attempted to use tool not available in profile"
        );
    }

    pub fn log_confirmation_required(
        tool_name: &str,
        profile_name: &str,
        user_id: Option<String>,
        conversation_id: Option<String>,
    ) {
        tracing::info!(
            target: "audit",
            tool = %tool_name,
            profile = %profile_name,
            user_id = ?user_id,
            conversation_id = ?conversation_id,
            "Tool execution requires user confirmation"
        );
    }
}
