use crate::llm::{Content, Llm, LlmRequest, LlmResponse, Part};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct VllmConfig {
    pub model: String,
    pub host: String,
}

impl VllmConfig {
    #[must_use]
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            host: "http://127.0.0.1:8000".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct VllmModel {
    config: VllmConfig,
    client: reqwest::Client,
}

impl VllmModel {
    pub fn new(config: VllmConfig) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            client: reqwest::Client::new(),
        })
    }
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<CompletionChoice>,
}

#[derive(Deserialize)]
struct CompletionChoice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

#[async_trait]
impl Llm for VllmModel {
    fn name(&self) -> &str {
        &self.config.model
    }

    async fn generate_content(&self, request: LlmRequest) -> anyhow::Result<LlmResponse> {
        let url = format!("{}/v1/chat/completions", self.config.host);

        // Extract text from contents
        let mut prompt = String::new();
        for content in &request.contents {
            for part in &content.parts {
                let Part::Text(text) = part;
                prompt.push_str(text);
            }
        }

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: prompt,
        }];

        let chat_request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages,
            temperature: 0.7,
            max_tokens: 2048,
            stop: Some(vec![
                "\n\nassistant\n".to_string(),
                "\n\n### ".to_string(),
                "\n\n**Additional".to_string(),
                "\n\nUser:".to_string(),
                "\n\nHuman:".to_string(),
            ]),
        };

        eprintln!("Calling vLLM: {url}");
        let response = self
            .client
            .post(&url)
            .json(&chat_request)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to call vLLM: {e}"))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("vLLM error: {error_text}"));
        }

        let chat_response: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse vLLM response: {e}"))?;

        let content = chat_response
            .choices
            .first()
            .map_or_else(|| "No response".to_string(), |c| c.message.content.clone());

        let llm_response = LlmResponse {
            content: Some(Content::new("assistant").with_text(content)),
        };

        Ok(llm_response)
    }
}
