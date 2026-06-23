use serde::{Deserialize, Serialize};
use std::fmt;

pub mod parser;
pub mod providers_duckduckgo;
pub mod providers_serpapi;
pub mod search_provider;
pub mod web_search;

pub use search_provider::{SearchProvider, SearchProviderRegistry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub parameters: ToolParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameters {
    #[serde(rename = "type")]
    pub param_type: String,
    pub properties: serde_json::Value,
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: String,
    pub is_error: bool,
}

impl fmt::Display for ToolResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.content)
    }
}

pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    vec![web_search::get_definition()]
}

pub fn get_tool_definitions_for_prompt() -> String {
    let tools = get_tool_definitions();
    match serde_json::to_string_pretty(&tools) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to serialize tool definitions: {}", e);
            String::new()
        }
    }
}

#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult;
}

pub struct ToolRegistry {
    executors: std::collections::HashMap<String, Box<dyn ToolExecutor>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            executors: std::collections::HashMap::new(),
        }
    }

    pub fn register(&mut self, name: String, executor: Box<dyn ToolExecutor>) {
        self.executors.insert(name, executor);
    }

    pub async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        match self.executors.get(&tool_call.name) {
            Some(executor) => executor.execute(tool_call).await,
            None => ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: format!("Tool '{}' not found", tool_call.name),
                is_error: true,
            },
        }
    }

    pub fn get_definitions(&self) -> Vec<ToolDefinition> {
        get_tool_definitions()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_valid_json() {
        let defs = get_tool_definitions_for_prompt();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&defs);
        assert!(parsed.is_ok(), "Tool definitions should be valid JSON");
    }
}
