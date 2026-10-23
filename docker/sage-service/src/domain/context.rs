use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextLimits {
    pub max_tokens: u32,
    pub warning_threshold_percent: u32, // e.g., 80 means warn at 80% full
}

impl ContextLimits {
    #[must_use]
    pub const fn new(max_tokens: u32, warning_threshold_percent: u32) -> Self {
        Self {
            max_tokens,
            warning_threshold_percent,
        }
    }

    #[must_use]
    pub const fn warning_token_count(&self) -> u32 {
        (self.max_tokens * self.warning_threshold_percent) / 100
    }

    #[must_use]
    pub const fn is_near_limit(&self, current_tokens: u32) -> bool {
        current_tokens >= self.warning_token_count()
    }

    #[must_use]
    pub const fn is_at_limit(&self, current_tokens: u32) -> bool {
        current_tokens >= self.max_tokens
    }

    #[must_use]
    pub const fn remaining_tokens(&self, current_tokens: u32) -> u32 {
        self.max_tokens.saturating_sub(current_tokens)
    }
}

#[must_use]
pub fn get_context_limits(profile: &str) -> ContextLimits {
    match profile {
        "code_assistant" | "cli_agent" => ContextLimits::new(8192, 75), // 8K token limit
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
    #[must_use]
    pub fn new(profile: &str, current_tokens: u32) -> Self {
        let limits = get_context_limits(profile);
        let max_tokens = limits.max_tokens;
        let remaining_tokens = limits.remaining_tokens(current_tokens);
        let usage_percent = (f64::from(current_tokens) / f64::from(max_tokens)) * 100.0;

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

/// Simple token counter estimating tokens from character count; a real implementation would use tiktoken or similar.
#[derive(Debug)]
pub struct TokenCounter;

impl TokenCounter {
    /// Rough estimation: ~1 token per 4 characters (varies by model); use a real tokenizer for accuracy.
    #[must_use]
    pub fn count_tokens(text: &str) -> u32 {
        u32::try_from(text.len().div_ceil(4)).unwrap_or(u32::MAX)
    }

    #[must_use]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContextWarningType {
    NearLimit,
    AtLimit,
}

#[derive(Debug)]
pub struct ContextManager;

impl ContextManager {
    #[must_use]
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
                    u32::try_from(u64::from(current_tokens) * 100 / u64::from(limits.max_tokens),)
                        .unwrap_or_default()
                ),
                current_tokens,
                max_tokens: limits.max_tokens,
            })
        } else {
            None
        }
    }

    /// Prune old messages to make room; returns the number of tokens freed.
    pub fn prune_messages(messages: &mut Vec<(String, String)>, target_tokens: u32) -> u32 {
        let mut freed_tokens = 0;

        // Keep system message (usually first), but remove others from the beginning
        let start_index = usize::from(!messages.is_empty() && messages[0].0 == "system");

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
