use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DefaultModel {
    pub name: String,
    #[serde(rename = "gpu_utilization")]
    pub gpu_memory_utilization: Option<f32>,
    #[serde(rename = "context_len")]
    pub max_model_len: Option<u32>,
}

impl DefaultModel {
    pub fn parse_list(input: &str) -> Vec<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // Try parsing as a JSON array of DefaultModel objects
        if let Ok(list) = serde_json::from_str::<Vec<DefaultModel>>(trimmed) {
            return list;
        }

        // Try parsing as a single JSON DefaultModel object
        if let Ok(single) = serde_json::from_str::<DefaultModel>(trimmed) {
            return vec![single];
        }

        // Backwards compatibility fallback for raw string name
        if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
            return vec![DefaultModel {
                name: trimmed.to_string(),
                gpu_memory_utilization: None,
                max_model_len: None,
            }];
        }

        tracing::error!("Failed to parse SAGE_DEFAULT_MODELS as JSON: {}", trimmed);
        Vec::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SageConfig {
    pub system_prompt: String,
    pub default_models: Vec<DefaultModel>,
    pub supported_models: Vec<String>,
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

        let default_models_str =
            envmnt::get_or("SAGE_DEFAULT_MODELS", r#"[{"name": "qwen2.5-coder:7b"}]"#);
        let default_models = DefaultModel::parse_list(&default_models_str);

        let supported_models_str =
            envmnt::get_or("SAGE_SUPPORTED_MODELS", "qwen*, *-instruct, llama*");
        let supported_models: Vec<String> = supported_models_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            system_prompt,
            default_models,
            supported_models,
        }
    }

    pub fn is_model_supported(&self, model: &str) -> bool {
        self.supported_models.iter().any(|pattern| {
            let regex_pattern = format!("(?i)^{}$", pattern.replace("*", ".*").replace("?", "."));
            if let Ok(re) = regex::Regex::new(&regex_pattern) {
                re.is_match(model)
            } else {
                false
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_default_models_json() {
        // Array of models
        let json_arr = r#"[
            {"name": "Qwen/Qwen2.5-0.5B-Instruct", "gpu_utilization": 0.20, "context_len": 32768},
            {"name": "llama3.1:8b", "gpu_utilization": 0.90}
        ]"#;
        let list = DefaultModel::parse_list(json_arr);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Qwen/Qwen2.5-0.5B-Instruct");
        assert_eq!(list[0].gpu_memory_utilization, Some(0.20));
        assert_eq!(list[0].max_model_len, Some(32768));
        assert_eq!(list[1].name, "llama3.1:8b");
        assert_eq!(list[1].gpu_memory_utilization, Some(0.90));
        assert_eq!(list[1].max_model_len, None);

        // Single JSON object
        let json_obj = r#"{"name": "qwen2.5-coder:7b", "context_len": 4096}"#;
        let list2 = DefaultModel::parse_list(json_obj);
        assert_eq!(list2.len(), 1);
        assert_eq!(list2[0].name, "qwen2.5-coder:7b");
        assert_eq!(list2[0].gpu_memory_utilization, None);
        assert_eq!(list2[0].max_model_len, Some(4096));

        // Backwards compatibility with raw string name
        let raw_str = "qwen2.5-coder:7b";
        let list3 = DefaultModel::parse_list(raw_str);
        assert_eq!(list3.len(), 1);
        assert_eq!(list3[0].name, "qwen2.5-coder:7b");
        assert_eq!(list3[0].gpu_memory_utilization, None);
        assert_eq!(list3[0].max_model_len, None);
    }
}
