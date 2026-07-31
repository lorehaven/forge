use super::{Content, Llm, LlmRequest, LlmResponse, Part};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Debug)]
pub struct OllamaModel {
    client: reqwest::Client,
    model: String,
    host: String,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    options: ChatOptions,
}

#[derive(Serialize)]
struct ChatOptions {
    temperature: f32,
    /// Ollama's name for max generated tokens.
    num_predict: usize,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

impl OllamaModel {
    pub fn new(model: impl Into<String>, host: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            model: model.into(),
            host: host.into(),
        })
    }
}

#[async_trait]
impl Llm for OllamaModel {
    fn name(&self) -> &str {
        &self.model
    }

    async fn generate_content(&self, request: LlmRequest) -> anyhow::Result<LlmResponse> {
        // Extract text from request contents
        let mut prompt = String::new();
        for content in &request.contents {
            for part in &content.parts {
                let Part::Text(text) = part;
                prompt.push_str(text);
            }
        }

        let chat_request = ChatRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            stream: false,
            options: ChatOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens,
            },
        };

        let url = format!("{}/api/chat", self.host);
        let response = self
            .client
            .post(&url)
            .json(&chat_request)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to call Ollama: {e}"))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Ollama error: {error_text}"));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse Ollama response: {e}"))?;

        let content = Content::new("assistant").with_text(chat_response.message.content);

        Ok(LlmResponse {
            content: Some(content),
        })
    }
}
