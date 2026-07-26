use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub calls_per_minute: u64,
    pub burst_limit: u64,
}

impl RateLimitConfig {
    pub fn new(calls_per_minute: u64, burst_limit: u64) -> Self {
        Self {
            calls_per_minute,
            burst_limit,
        }
    }
}

pub struct RateLimiter {
    // Profile -> Config
    configs: std::collections::HashMap<String, RateLimitConfig>,
    // User ID -> VecDeque of call timestamps
    user_calls: Arc<DashMap<String, VecDeque<Instant>>>,
    // Conversation ID -> VecDeque of call timestamps
    conversation_calls: Arc<DashMap<String, VecDeque<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        let mut configs = std::collections::HashMap::new();

        // Default rate limits per profile
        configs.insert(
            "web_assistant".to_string(),
            RateLimitConfig::new(120, 30), // 120/min, burst of 30
        );
        configs.insert(
            "code_assistant".to_string(),
            RateLimitConfig::new(60, 15), // 60/min, burst of 15
        );
        configs.insert(
            "cli_agent".to_string(),
            RateLimitConfig::new(30, 5), // 30/min, burst of 5
        );

        Self {
            configs,
            user_calls: Arc::new(DashMap::new()),
            conversation_calls: Arc::new(DashMap::new()),
        }
    }

    pub fn check_rate_limit(
        &self,
        profile: &str,
        user_id: Option<&str>,
        conversation_id: Option<&str>,
    ) -> Result<(), RateLimitError> {
        let config = self
            .configs
            .get(profile)
            .cloned()
            .unwrap_or_else(|| RateLimitConfig::new(60, 15));

        let now = Instant::now();
        let minute_ago = now - Duration::from_secs(60);

        // Check user rate limit
        if let Some(user_id) = user_id {
            let mut calls = self.user_calls.entry(user_id.to_string()).or_default();

            // Remove old calls outside the 1-minute window
            while let Some(&call_time) = calls.front() {
                if call_time < minute_ago {
                    calls.pop_front();
                } else {
                    break;
                }
            }

            // Check if we've exceeded the limit
            if calls.len() >= config.calls_per_minute as usize {
                return Err(RateLimitError::UserRateLimitExceeded {
                    limit: config.calls_per_minute,
                    user_id: user_id.to_string(),
                });
            }

            calls.push_back(now);
        }

        // Check conversation rate limit (burst protection)
        if let Some(conversation_id) = conversation_id {
            let mut calls = self
                .conversation_calls
                .entry(conversation_id.to_string())
                .or_default();

            // Remove old calls
            while let Some(&call_time) = calls.front() {
                if call_time < minute_ago {
                    calls.pop_front();
                } else {
                    break;
                }
            }

            // Check burst limit
            if calls.len() >= config.burst_limit as usize {
                return Err(RateLimitError::BurstLimitExceeded {
                    limit: config.burst_limit,
                    conversation_id: conversation_id.to_string(),
                });
            }

            calls.push_back(now);
        }

        Ok(())
    }

    pub fn set_config(&mut self, profile: &str, config: RateLimitConfig) {
        self.configs.insert(profile.to_string(), config);
    }

    pub fn get_config(&self, profile: &str) -> Option<RateLimitConfig> {
        self.configs.get(profile).cloned()
    }

    pub fn reset_user(&self, user_id: &str) {
        self.user_calls.remove(user_id);
    }

    pub fn reset_conversation(&self, conversation_id: &str) {
        self.conversation_calls.remove(conversation_id);
    }

    pub fn reset_all(&self) {
        self.user_calls.clear();
        self.conversation_calls.clear();
    }

    pub fn get_user_remaining_calls(&self, profile: &str, user_id: &str) -> u64 {
        let config = self
            .configs
            .get(profile)
            .cloned()
            .unwrap_or_else(|| RateLimitConfig::new(60, 15));

        let calls = self
            .user_calls
            .get(user_id)
            .map(|c| c.len() as u64)
            .unwrap_or(0);

        config.calls_per_minute.saturating_sub(calls)
    }

    pub fn get_conversation_remaining_calls(&self, profile: &str, conversation_id: &str) -> u64 {
        let config = self
            .configs
            .get(profile)
            .cloned()
            .unwrap_or_else(|| RateLimitConfig::new(60, 15));

        let calls = self
            .conversation_calls
            .get(conversation_id)
            .map(|c| c.len() as u64)
            .unwrap_or(0);

        config.burst_limit.saturating_sub(calls)
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum RateLimitError {
    UserRateLimitExceeded { limit: u64, user_id: String },
    BurstLimitExceeded { limit: u64, conversation_id: String },
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitError::UserRateLimitExceeded { limit, user_id } => {
                write!(
                    f,
                    "User '{}' has exceeded rate limit of {} calls per minute",
                    user_id, limit
                )
            }
            RateLimitError::BurstLimitExceeded {
                limit,
                conversation_id,
            } => {
                write!(
                    f,
                    "Conversation '{}' has exceeded burst limit of {} concurrent calls",
                    conversation_id, limit
                )
            }
        }
    }
}
