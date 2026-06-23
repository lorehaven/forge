use anyhow::{Context, Result};
use futures_util::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionChunk {
    pub choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionChoice {
    pub delta: ChatCompletionDelta,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionDelta {
    pub content: Option<String>,
}

#[derive(Clone)]
pub struct VllmClient {
    http: reqwest::Client,
}

impl VllmClient {
    pub fn new() -> Self {
        let tls_verify = envmnt::get_or("VLLM_TLS_VERIFY", "true")
            .parse::<bool>()
            .unwrap_or(true);

        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(!tls_verify)
            .build()
            .expect("Failed to build HTTP client");

        Self { http }
    }

    pub async fn chat_stream(
        &self,
        host: &str,
        port: u16,
        model: &str,
        messages: Vec<ChatMessage>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let host = host
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let url = format!("http://{}:{}/v1/chat/completions", host, port);

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages,
            stream: true,
            temperature: Some(0.1), // LOW TEMPERATURE FOR PRECISION
            top_p: Some(0.9),
            presence_penalty: Some(0.0),
            frequency_penalty: Some(0.0),
            max_tokens,
        };

        tracing::info!(
            "Connecting to vLLM at {} (max_tokens: {:?})",
            url,
            max_tokens
        );

        let res = self
            .http
            .post(&url)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("Failed to connect to vLLM instance at {}", url))?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            tracing::error!("vLLM returned error {}: {}", status, err_text);
            anyhow::bail!("vLLM returned error {}: {}", status, err_text);
        }

        let mut stream = res.bytes_stream();

        let output_stream = async_stream::try_stream! {
            let mut buffer = Vec::new();

            while let Some(chunk_res) = stream.next().await {
                let chunk = chunk_res.context("Error reading from vLLM stream")?;
                buffer.extend_from_slice(&chunk);

                while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                    let line_bytes = buffer.drain(..=pos).collect::<Vec<u8>>();
                    let line_str = String::from_utf8_lossy(&line_bytes);
                    let line = line_str.trim();

                    if line.is_empty() { continue; }
                    if line == "data: [DONE]" {
                        return;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(data) {
                            if let Some(content) = &chunk.choices[0].delta.content {
                                yield content.clone();
                            }
                        } else {
                            tracing::warn!("Failed to parse SSE data: {}", data);
                        }
                    }
                }
            }
        };

        Ok(Box::pin(output_stream))
    }
}
