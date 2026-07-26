use crate::clients::vllm::ChatMessage;
use crate::domain::conversation::{ConversationContext, ConversationMessage};
use quench_db::prelude::Crud;

/// Build conversation context from database messages
pub async fn build_conversation_context(
    db: &actix_web::web::Data<quench_db::prelude::Db>,
    conversation_id: &str,
    max_context_tokens: u32,
) -> Result<ConversationContext, String> {
    use crate::domain::models::Message;

    let mut ctx = ConversationContext::new(max_context_tokens);

    let msg_repo = db.repository::<Message>();
    let messages = msg_repo
        .list()
        .await
        .map_err(|e| format!("Failed to fetch messages: {}", e))?;

    // Filter messages for this conversation
    let mut conv_messages: Vec<_> = messages
        .into_iter()
        .filter(|m| m.conversation_id == conversation_id)
        .collect();

    // Sort by creation time
    conv_messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    // Add to context
    for msg in conv_messages {
        // Parse created_at from ISO 8601 string
        let created_at = chrono::DateTime::parse_from_rfc3339(&msg.created_at)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        let conv_msg = ConversationMessage {
            id: msg.id,
            role: msg.role,
            content: msg.content,
            parent_id: msg.parent_id,
            created_at,
        };
        ctx.add_message(conv_msg);
    }

    Ok(ctx)
}

/// Get context messages for LLM with token budgeting
pub fn get_context_for_llm(
    ctx: &ConversationContext,
    system_prompt: &str,
) -> (Vec<ChatMessage>, TokenUsageInfo) {
    let (messages, _total_tokens) = ctx.get_context_messages(system_prompt);
    let stats = ctx.get_token_stats();

    let usage = TokenUsageInfo {
        total_messages: stats.total_messages,
        total_tokens: stats.total_tokens,
        max_tokens: stats.max_tokens,
        system_tokens: ConversationContext::estimate_tokens(system_prompt),
        near_limit: stats.is_near_limit(),
        at_limit: stats.is_at_limit(),
        utilization_percent: stats.token_utilization,
    };

    (messages, usage)
}

#[derive(Debug, Clone)]
pub struct TokenUsageInfo {
    pub total_messages: usize,
    pub total_tokens: u32,
    pub max_tokens: u32,
    pub system_tokens: u32,
    pub near_limit: bool,
    pub at_limit: bool,
    pub utilization_percent: f32,
}

impl TokenUsageInfo {
    pub fn format_display(&self) -> String {
        format!(
            "Tokens: {}/{} ({:.1}%) | Messages: {} | System: {}",
            self.total_tokens,
            self.max_tokens,
            self.utilization_percent,
            self.total_messages,
            self.system_tokens
        )
    }

    pub fn warning_message(&self) -> Option<String> {
        if self.at_limit {
            Some("⚠️ Context window at capacity - consider starting a new conversation".to_string())
        } else if self.near_limit {
            Some("⚠️ Context window 80% full - longer responses may be truncated".to_string())
        } else {
            None
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "total_messages": self.total_messages,
            "total_tokens": self.total_tokens,
            "max_tokens": self.max_tokens,
            "system_tokens": self.system_tokens,
            "utilization_percent": self.utilization_percent,
            "near_limit": self.near_limit,
            "at_limit": self.at_limit
        })
    }
}
