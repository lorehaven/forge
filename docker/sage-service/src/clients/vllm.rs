use anyhow::{Context, Result};
use futures_util::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
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
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        let url = format!("http://{}:{}/v1/chat/completions", host, port);

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages,
            stream: true,
        };

        let res = self
            .http
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to connect to vLLM instance")?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res.text().await.unwrap_or_default();
            anyhow::bail!("vLLM returned error {}: {}", status, err_text);
        }

        let mut stream = res.bytes_stream();

        let output_stream = async_stream::try_stream! {
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.context("Error reading from vLLM stream")?;
                let text = String::from_utf8_lossy(&chunk);
                buffer.push_str(&text);

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer.drain(..=pos).collect::<String>();
                    let line = line.trim();

                    if line.is_empty() { continue; }
                    if line == "data: [DONE]" { break; }

                    if let Some(data) = line.strip_prefix("data: ")
                        && let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(data)
                        && let Some(content) = &chunk.choices[0].delta.content {
                        yield content.clone();
                    }
                }
            }
        };

        Ok(Box::pin(output_stream))
    }
}
