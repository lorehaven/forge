pub mod ollama;

use async_trait::async_trait;
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub enum Part {
    Text(String),
}

#[derive(Debug, Clone)]
pub struct Content {
    pub role: String,
    pub parts: Vec<Part>,
}

impl Content {
    #[must_use]
    pub fn new(role: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            parts: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.parts.push(Part::Text(text.into()));
        self
    }
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub model: String,
    pub contents: Vec<Content>,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Option<Content>,
}

#[async_trait]
pub trait Llm: Send + Sync + Debug {
    fn name(&self) -> &str;
    async fn generate_content(&self, request: LlmRequest) -> anyhow::Result<LlmResponse>;
}
