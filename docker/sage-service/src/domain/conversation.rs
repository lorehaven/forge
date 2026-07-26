use crate::clients::vllm::ChatMessage;

/// Manages conversation context with token budgeting
pub struct ConversationContext {
    /// Maximum tokens for context window
    pub max_context_tokens: u32,
    /// Estimate of tokens per character (rough approximation)
    pub tokens_per_char: f32,
    /// Messages in the conversation history
    pub messages: Vec<ConversationMessage>,
}

#[derive(Clone, Debug)]
pub struct ConversationMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub parent_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ConversationContext {
    /// Create new conversation context
    pub fn new(max_context_tokens: u32) -> Self {
        Self {
            max_context_tokens,
            tokens_per_char: 0.25, // Rough estimate: 1 token ≈ 4 characters
            messages: Vec::new(),
        }
    }

    /// Add a message to conversation history
    pub fn add_message(&mut self, msg: ConversationMessage) {
        self.messages.push(msg);
    }

    /// Estimate tokens in text
    pub fn estimate_tokens(text: &str) -> u32 {
        (text.len() as f32 * 0.25).ceil() as u32
    }

    /// Get messages for context with token budget consideration
    pub fn get_context_messages(&self, system_prompt: &str) -> (Vec<ChatMessage>, u32) {
        let mut total_tokens = Self::estimate_tokens(system_prompt);
        let mut context_messages = Vec::new();

        // System message
        context_messages.push(ChatMessage {
            role: "system".to_string(),
            content: system_prompt.to_string(),
            tool_calls: None,
            images: None,
        });

        // Add messages in reverse chronological order (newest first)
        // but insert them in chronological order
        let mut messages_to_include = Vec::new();
        for msg in self.messages.iter().rev() {
            let msg_tokens = Self::estimate_tokens(&msg.content);
            if total_tokens + msg_tokens > self.max_context_tokens {
                // Context window exceeded, stop adding messages
                break;
            }
            total_tokens += msg_tokens;
            messages_to_include.push(msg.clone());
        }

        // Reverse to get chronological order
        messages_to_include.reverse();

        // Convert to ChatMessage format
        for msg in messages_to_include {
            context_messages.push(ChatMessage {
                role: msg.role,
                content: msg.content,
                tool_calls: None,
                images: None,
            });
        }

        (context_messages, total_tokens)
    }

    /// Get conversation summary (for display)
    pub fn get_messages_by_id(&self, message_id: Option<&str>) -> Vec<&ConversationMessage> {
        if let Some(id) = message_id {
            // Collect the message and all of its ancestors, ordered root-first.
            // This mirrors the branch semantics used when switching threads:
            // a "branch" is the path from the root down to the selected message.
            let mut result = Vec::new();
            let mut visited = std::collections::HashSet::new();
            let mut current = Some(id);
            while let Some(cid) = current {
                if !visited.insert(cid) {
                    break; // guard against cycles
                }
                let Some(msg) = self.messages.iter().find(|m| m.id == cid) else {
                    break;
                };
                result.push(msg);
                current = msg.parent_id.as_deref();
            }
            result.reverse();
            result
        } else {
            // Return all messages in a linear path (following parent relationships)
            self.get_main_branch()
        }
    }

    fn get_main_branch(&self) -> Vec<&ConversationMessage> {
        let mut result = Vec::new();

        // Find root (message with no parent)
        if let Some(root) = self.messages.iter().find(|m| m.parent_id.is_none()) {
            self.follow_branch(root, &mut result);
        }

        result
    }

    fn follow_branch<'a>(
        &'a self,
        msg: &'a ConversationMessage,
        result: &mut Vec<&'a ConversationMessage>,
    ) {
        result.push(msg);

        // Find the first child (could have multiple branches)
        if let Some(child) = self
            .messages
            .iter()
            .find(|m| m.parent_id.as_deref() == Some(&msg.id))
        {
            self.follow_branch(child, result);
        }
    }

    /// Get token usage stats
    pub fn get_token_stats(&self) -> TokenStats {
        let total_tokens = Self::estimate_tokens(
            &self
                .messages
                .iter()
                .map(|m| &m.content[..])
                .collect::<Vec<_>>()
                .join(""),
        );

        TokenStats {
            total_messages: self.messages.len(),
            total_tokens,
            max_tokens: self.max_context_tokens,
            token_utilization: if self.max_context_tokens > 0 {
                (total_tokens as f32 / self.max_context_tokens as f32) * 100.0
            } else {
                0.0
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct TokenStats {
    pub total_messages: usize,
    pub total_tokens: u32,
    pub max_tokens: u32,
    pub token_utilization: f32,
}

impl TokenStats {
    pub fn is_near_limit(&self) -> bool {
        self.token_utilization > 80.0
    }

    pub fn is_at_limit(&self) -> bool {
        self.token_utilization > 95.0
    }
}
