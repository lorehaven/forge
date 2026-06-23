use super::{
    SearchProviderRegistry, ToolCall, ToolDefinition, ToolExecutor, ToolParameters, ToolResult,
};
use serde_json::json;
use std::sync::Arc;

pub fn get_definition() -> ToolDefinition {
    ToolDefinition {
        name: "web_search".to_string(),
        description: "Search the web using DuckDuckGo to find current information, news, and resources. Use this when you need up-to-date information beyond your training data.".to_string(),
        tool_type: "function".to_string(),
        parameters: ToolParameters {
            param_type: "object".to_string(),
            properties: json!({
                "query": {
                    "type": "string",
                    "description": "The search query to look up"
                }
            }),
            required: vec!["query".to_string()],
        },
    }
}

pub struct WebSearchExecutor {
    provider_registry: Arc<SearchProviderRegistry>,
    default_provider: String,
}

impl WebSearchExecutor {
    pub fn new(provider_registry: Arc<SearchProviderRegistry>) -> Self {
        Self {
            provider_registry,
            default_provider: "duckduckgo".to_string(),
        }
    }

    pub fn with_default_provider(mut self, provider: String) -> Self {
        self.default_provider = provider;
        self
    }
}

#[async_trait::async_trait]
impl ToolExecutor for WebSearchExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        let query = match tool_call.arguments.get("query") {
            Some(serde_json::Value::String(q)) => q.clone(),
            _ => {
                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: "Missing or invalid 'query' parameter".to_string(),
                    is_error: true,
                };
            }
        };

        let provider = self
            .provider_registry
            .get(Some(&self.default_provider))
            .or_else(|| self.provider_registry.get(None))
            .unwrap_or_else(|| self.provider_registry.get_default());

        match provider.search(&query).await {
            Ok(results) => ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: results,
                is_error: false,
            },
            Err(e) => ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: format!("Web search failed: {}", e),
                is_error: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_search_definition() {
        let def = get_definition();
        assert_eq!(def.name, "web_search");
        assert!(!def.description.is_empty());
        assert_eq!(def.parameters.param_type, "object");
        assert!(def.parameters.required.contains(&"query".to_string()));
    }
}
