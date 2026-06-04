use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageConfig {
    pub system_prompt: String,
}

impl SageConfig {
    pub fn load() -> Self {
        let prompt_path = envmnt::get_or("SAGE_SYSTEM_PROMPT_PATH", "config/system_prompt.txt");

        let system_prompt = match fs::read_to_string(&prompt_path) {
            Ok(content) => content.trim().to_string(),
            Err(err) => {
                tracing::warn!(
                    "Failed to read system prompt from {}: {}. Using default.",
                    prompt_path,
                    err
                );
                "You are Sage, a wise and helpful AI assistant.".to_string()
            }
        };

        Self { system_prompt }
    }
}
