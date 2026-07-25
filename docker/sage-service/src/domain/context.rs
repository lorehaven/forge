use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextLimits {
    pub max_tokens: u32,
    pub warning_threshold_percent: u32, // e.g., 80 means warn at 80% full
}

impl ContextLimits {
    pub fn new(max_tokens: u32, warning_threshold_percent: u32) -> Self {
        Self {
            max_tokens,
            warning_threshold_percent,
        }
    }

    pub fn warning_token_count(&self) -> u32 {
        (self.max_tokens * self.warning_threshold_percent) / 100
    }

    pub fn is_near_limit(&self, current_tokens: u32) -> bool {
        current_tokens >= self.warning_token_count()
    }

    pub fn is_at_limit(&self, current_tokens: u32) -> bool {
        current_tokens >= self.max_tokens
    }

    pub fn remaining_tokens(&self, current_tokens: u32) -> u32 {
        self.max_tokens.saturating_sub(current_tokens)
    }
}

pub fn get_context_limits(profile: &str) -> ContextLimits {
    match profile {
        "web_assistant" => ContextLimits::new(4096, 80), // 4K token limit
        "code_assistant" => ContextLimits::new(8192, 75), // 8K token limit
        "cli_agent" => ContextLimits::new(8192, 75),     // 8K token limit
        _ => ContextLimits::new(4096, 80),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStatus {
    pub profile: String,
    pub current_tokens: u32,
    pub max_tokens: u32,
    pub remaining_tokens: u32,
    pub usage_percent: f64,
    pub is_near_limit: bool,
    pub is_at_limit: bool,
}

impl ContextStatus {
    pub fn new(profile: &str, current_tokens: u32) -> Self {
        let limits = get_context_limits(profile);
        let max_tokens = limits.max_tokens;
        let remaining_tokens = limits.remaining_tokens(current_tokens);
        let usage_percent = (current_tokens as f64 / max_tokens as f64) * 100.0;

        Self {
            profile: profile.to_string(),
            current_tokens,
            max_tokens,
            remaining_tokens,
            usage_percent,
            is_near_limit: limits.is_near_limit(current_tokens),
            is_at_limit: limits.is_at_limit(current_tokens),
        }
    }
}

/// Simple token counter - estimates tokens based on character count
/// Real implementation would use tiktoken or similar
pub struct TokenCounter;

impl TokenCounter {
    /// Rough estimation: ~1 token per 4 characters (varies by model)
    /// For accuracy, use actual tokenizer (tiktoken for OpenAI)
    pub fn count_tokens(text: &str) -> u32 {
        (text.len() as u32).div_ceil(4)
    }

    pub fn count_tokens_for_messages(messages: &[(String, String)]) -> u32 {
        let mut total = 0;
        for (role, content) in messages {
            // ~1 token for role, content tokens, plus ~4 overhead per message
            total += Self::count_tokens(role) + Self::count_tokens(content) + 4;
        }
        total
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWarning {
    pub warning_type: ContextWarningType,
    pub message: String,
    pub current_tokens: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContextWarningType {
    NearLimit,
    AtLimit,
}

pub struct ContextManager;

impl ContextManager {
    pub fn check_context(profile: &str, current_tokens: u32) -> Option<ContextWarning> {
        let limits = get_context_limits(profile);

        if limits.is_at_limit(current_tokens) {
            Some(ContextWarning {
                warning_type: ContextWarningType::AtLimit,
                message: format!(
                    "Context window is at maximum ({} tokens). Conversation history may need to be pruned.",
                    limits.max_tokens
                ),
                current_tokens,
                max_tokens: limits.max_tokens,
            })
        } else if limits.is_near_limit(current_tokens) {
            Some(ContextWarning {
                warning_type: ContextWarningType::NearLimit,
                message: format!(
                    "Context window is nearly full: {}/{} tokens used ({}%). Consider pruning old messages.",
                    current_tokens,
                    limits.max_tokens,
                    (current_tokens as f64 / limits.max_tokens as f64 * 100.0) as u32
                ),
                current_tokens,
                max_tokens: limits.max_tokens,
            })
        } else {
            None
        }
    }

    /// Prune old messages to make room
    /// Returns number of tokens freed
    pub fn prune_messages(messages: &mut Vec<(String, String)>, target_tokens: u32) -> u32 {
        let mut freed_tokens = 0;

        // Keep system message (usually first), but remove others from the beginning
        let start_index = if !messages.is_empty() && messages[0].0 == "system" {
            1
        } else {
            0
        };

        while messages.len() > start_index + 1 {
            let msg = messages.remove(start_index);
            let msg_tokens = TokenCounter::count_tokens(&msg.1);
            freed_tokens += msg_tokens;

            if freed_tokens >= target_tokens {
                break;
            }
        }

        freed_tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_limits() {
        let limits = get_context_limits("web_assistant");
        assert_eq!(limits.max_tokens, 4096);
        assert_eq!(limits.warning_threshold_percent, 80);
    }

    #[test]
    fn test_token_counting() {
        // ~1 token per 4 characters
        let tokens = TokenCounter::count_tokens("hello world"); // 11 chars -> ~3 tokens
        assert!((2..=4).contains(&tokens));
    }

    #[test]
    fn test_context_status() {
        let status = ContextStatus::new("web_assistant", 3280);
        assert_eq!(status.max_tokens, 4096);
        assert_eq!(status.remaining_tokens, 816);
        assert!(status.is_near_limit);
        assert!(!status.is_at_limit);
        assert!((status.usage_percent - 80.0).abs() < 1.0);
    }

    #[test]
    fn test_context_warning() {
        // Near limit
        let warning = ContextManager::check_context("web_assistant", 3500);
        assert!(warning.is_some());
        assert_eq!(warning.unwrap().warning_type, ContextWarningType::NearLimit);

        // At limit
        let warning = ContextManager::check_context("web_assistant", 4096);
        assert!(warning.is_some());
        assert_eq!(warning.unwrap().warning_type, ContextWarningType::AtLimit);

        // Safe
        let warning = ContextManager::check_context("web_assistant", 2000);
        assert!(warning.is_none());
    }

    #[test]
    fn test_prune_messages() {
        let mut messages = vec![
            ("system".to_string(), "You are helpful".to_string()),
            ("user".to_string(), "Hello there".to_string()),
            ("assistant".to_string(), "Hi! How can I help?".to_string()),
            ("user".to_string(), "Tell me a joke".to_string()),
        ];

        let freed = ContextManager::prune_messages(&mut messages, 50);
        assert!(freed > 0);
        assert_eq!(messages[0].0, "system"); // System message preserved
        assert!(messages.len() < 4); // Some messages removed
    }
}
