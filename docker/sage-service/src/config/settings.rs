use quench_config::ConfigLoader;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DefaultModel {
    pub name: String,
    #[serde(rename = "gpu_utilization")]
    pub gpu_memory_utilization: Option<f32>,
    #[serde(rename = "context_len")]
    pub max_model_len: Option<u32>,
    /// vLLM quantization method (`--quantization`, e.g. "awq"); None lets vLLM infer from the checkpoint.
    #[serde(rename = "quant", default)]
    pub quantization: Option<String>,
    /// vLLM dtype (`--dtype`, e.g. "float16" for models that fail under default bfloat16); None = vLLM "auto".
    #[serde(default)]
    pub dtype: Option<String>,
    /// Multimodal input limit for vision models, passed verbatim as `--limit-mm-per-prompt`
    /// (e.g. `{"image": 4}`); should cover SAGE_MAX_IMAGES_PER_REQUEST. None = vLLM default (1 per modality).
    #[serde(rename = "limit_mm", default)]
    pub limit_mm_per_prompt: Option<String>,
    #[serde(default)]
    pub enable_tool_calling: bool,
    /// vLLM task, e.g. "embed", so switchboard serves /v1/embeddings instead of chat completions.
    #[serde(default)]
    pub task: Option<String>,
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
                quantization: None,
                dtype: None,
                limit_mm_per_prompt: None,
                enable_tool_calling: false,
                task: None,
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
    pub default_search_provider: String,
    pub available_search_providers: Vec<String>,
    pub capability_profile: crate::tools::CapabilityProfile,
    /// When true, default models launched at startup are gracefully stopped (SIGTERM via
    /// switchboard) on shutdown. Controlled by `SAGE_STOP_MODELS_ON_SHUTDOWN` (default: false).
    pub stop_models_on_shutdown: bool,
}

impl SageConfig {
    pub fn load() -> Self {
        let loader = ConfigLoader::new("SAGE");
        let prompt_path = loader.env_string("SYSTEM_PROMPT_PATH", "config/system_prompt.txt");

        // Load capability profile
        let profile_name = loader.env_string("CAPABILITY_PROFILE", "web_assistant");
        let capability_profile = crate::tools::capabilities::get_profile(&profile_name)
            .unwrap_or_else(|| {
                tracing::warn!(
                    "Unknown capability profile '{}', falling back to 'web_assistant'",
                    profile_name
                );
                crate::tools::capabilities::get_profile("web_assistant").unwrap()
            });

        tracing::info!(
            "Loaded capability profile '{}': {}",
            capability_profile.name,
            capability_profile.description
        );

        let mut system_prompt = match fs::read_to_string(&prompt_path) {
            Ok(content) => content.trim().to_string(),
            Err(err) => {
                tracing::warn!(
                    "Failed to read system prompt from {}: {}. Using default.",
                    prompt_path,
                    err
                );
                "You are Sage, a wise and helpful AI assistant. Always respond in the language of the user's most recent message.".to_string()
            }
        };

        // Append tool definitions to system prompt (filtered by profile)
        let tools_def = crate::tools::get_tool_definitions_for_prompt_filtered(&capability_profile);
        let tool_defs_count =
            crate::tools::get_tool_definitions_filtered(&capability_profile).len();

        if !tools_def.is_empty() {
            system_prompt.push_str("\n\n### AVAILABLE TOOLS\n");
            system_prompt.push_str("You have access to the following tools. When you need to use a tool, format your response with tool_call XML tags:\n\n");
            system_prompt.push_str(&tools_def);
            system_prompt.push_str("\n\nWhen calling tools, use this format:\n");
            system_prompt.push_str("<toolcall>{\"type\": \"function\", \"function\": {\"name\": \"web_search\", \"arguments\": {\"query\": \"your search query\"}}}</toolcall>\n");
            system_prompt.push_str("or\n");
            system_prompt.push_str("<toolcall>{\"type\": \"search\", \"name\": \"calculator\", \"arguments\": {\"expression\": \"2 + 2\"}}</toolcall>\n");
            system_prompt
                .push_str("\nAlways use the exact tool names from the definitions above.\n");
            tracing::info!(
                "[CONFIG] Loaded {} tool definitions into system prompt ({} enabled tools)",
                tool_defs_count,
                capability_profile.enabled_tool_names().len()
            );
            tracing::debug!("[CONFIG] Tool definitions:\n{}", tools_def);
        } else {
            tracing::warn!(
                "[CONFIG] No tool definitions loaded! Profile has {} enabled tools but JSON serialization returned empty",
                capability_profile.enabled_tool_names().len()
            );
        }

        // Model definitions come from SAGE_DEFAULT_MODELS (see docker/sage-service/.env).
        let default_models_str =
            loader.env_string("DEFAULT_MODELS", r#"[{"name": "qwen2.5-coder:7b"}]"#);
        let default_models = DefaultModel::parse_list(&default_models_str);

        let supported_models_str =
            loader.env_string("SUPPORTED_MODELS", "qwen*, *-instruct, llama*");
        let supported_models: Vec<String> = supported_models_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let default_search_provider = loader.env_string("SEARCH_PROVIDER", "duckduckgo");

        let stop_models_on_shutdown = loader.env_bool("STOP_MODELS_ON_SHUTDOWN", false);

        // Build list of available search providers
        let mut available_search_providers = vec!["duckduckgo".to_string(), "searxng".to_string()];
        if std::env::var("BRAVE_SEARCH_API_KEY").is_ok() {
            available_search_providers.push("brave".to_string());
        }
        if std::env::var("SERPAPI_API_KEY").is_ok() {
            available_search_providers.push("serpapi".to_string());
        }
        available_search_providers.sort();

        Self {
            system_prompt,
            default_models,
            supported_models,
            default_search_provider,
            available_search_providers,
            capability_profile,
            stop_models_on_shutdown,
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
