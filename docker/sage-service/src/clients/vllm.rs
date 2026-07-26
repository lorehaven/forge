use anyhow::{Context, Result};
use futures_util::Stream;
use futures_util::StreamExt;
use quench_starter::metrics::RequestMetrics;
use quench_starter::resilience::CircuitBreaker;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Image attachments as data URIs (`data:image/png;base64,...`). When
    /// present, the message is serialized as an OpenAI content-parts array so
    /// vision models receive the images alongside the text.
    #[serde(default)]
    pub images: Option<Vec<String>>,
}

/// Serialize to the OpenAI chat format: plain string `content` for text-only
/// messages, an array of `text` / `image_url` parts when images are attached.
impl Serialize for ChatMessage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        let has_images = self.images.as_ref().is_some_and(|v| !v.is_empty());
        // Images fold into `content`, so the field count never includes them.
        let fields = 2 + usize::from(self.tool_calls.is_some());
        let mut state = serializer.serialize_struct("ChatMessage", fields)?;
        state.serialize_field("role", &self.role)?;
        if has_images {
            let mut parts: Vec<serde_json::Value> = Vec::new();
            if !self.content.is_empty() {
                parts.push(serde_json::json!({"type": "text", "text": self.content}));
            }
            for url in self.images.iter().flatten() {
                parts.push(serde_json::json!({"type": "image_url", "image_url": {"url": url}}));
            }
            state.serialize_field("content", &parts)?;
        } else {
            state.serialize_field("content", &self.content)?;
        }
        if let Some(tool_calls) = &self.tool_calls {
            state.serialize_field("tool_calls", tool_calls)?;
        }
        state.end()
    }
}

#[derive(Debug, Clone, Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionChunk {
    pub choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChatCompletionChoice {
    pub delta: ChatCompletionDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChatCompletionDelta {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<StreamingToolCall>>,
}

/// One entry of a streamed OpenAI `tool_calls` delta. vLLM emits these
/// incrementally: the first chunk for a call carries `function.name`, later
/// chunks append `function.arguments` fragments, all keyed by `index`.
#[derive(Debug, Deserialize, Clone)]
pub struct StreamingToolCall {
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub function: Option<StreamingFunction>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct StreamingFunction {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

/// Convert accumulated streamed tool calls into `<tool_call>` text lines that
/// the text-based tool parser understands, so native OpenAI function calling
/// flows through the same downstream pipeline as tag-formatted calls. Drains
/// the accumulator; entries without a name are skipped.
pub fn flush_streamed_tool_calls(
    accum: &mut std::collections::BTreeMap<usize, (String, String)>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (_index, (name, args)) in std::mem::take(accum) {
        if name.is_empty() {
            continue;
        }
        // Arguments arrive as a JSON string (e.g. "{}" or "{\"q\":\"x\"}");
        // embed verbatim, defaulting to an empty object when absent.
        let args = if args.trim().is_empty() {
            "{}".to_string()
        } else {
            args
        };
        let name_json = serde_json::to_string(&name).unwrap_or_else(|_| format!("\"{}\"", name));
        out.push(format!(
            "<tool_call>{{\"name\": {}, \"arguments\": {}}}</tool_call>",
            name_json, args
        ));
    }
    out
}

#[derive(Clone)]
pub struct VllmClient {
    http: reqwest::Client,
    metrics: Arc<RequestMetrics>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl Default for VllmClient {
    fn default() -> Self {
        Self::new()
    }
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

        Self {
            http,
            metrics: Arc::new(RequestMetrics::new()),
            circuit_breaker: Arc::new(CircuitBreaker::new(3, 2, 60)),
        }
    }

    pub fn metrics(&self) -> Arc<RequestMetrics> {
        self.metrics.clone()
    }

    pub async fn chat_stream(
        &self,
        host: &str,
        port: u16,
        model: &str,
        messages: Vec<ChatMessage>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        self.chat_stream_with_tools(host, port, model, messages, max_tokens, None)
            .await
    }

    pub async fn chat_stream_with_tools(
        &self,
        host: &str,
        port: u16,
        model: &str,
        messages: Vec<ChatMessage>,
        max_tokens: Option<u32>,
        tools: Option<Vec<serde_json::Value>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        if !self.circuit_breaker.is_available() {
            tracing::warn!("vLLM circuit breaker is open");
            return Err(anyhow::anyhow!(
                "Circuit breaker open: vLLM temporarily unavailable"
            ));
        }

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
            tools,
            tool_choice: None,
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
            self.circuit_breaker.call_failed();
            anyhow::bail!("vLLM returned error {}: {}", status, err_text);
        }

        self.circuit_breaker.call_succeeded();
        let mut stream = res.bytes_stream();

        let output_stream = async_stream::try_stream! {
            let mut buffer = Vec::new();
            // Accumulates native (OpenAI) streamed tool calls by index until the
            // model signals completion, then re-emits them as <tool_call> text.
            let mut tool_accum: std::collections::BTreeMap<usize, (String, String)> =
                std::collections::BTreeMap::new();

            while let Some(chunk_res) = stream.next().await {
                let chunk = chunk_res.context("Error reading from vLLM stream")?;
                buffer.extend_from_slice(&chunk);

                while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                    let line_bytes = buffer.drain(..=pos).collect::<Vec<u8>>();
                    let line_str = String::from_utf8_lossy(&line_bytes);
                    let line = line_str.trim();

                    if line.is_empty() { continue; }
                    if line == "data: [DONE]" {
                        for synth in flush_streamed_tool_calls(&mut tool_accum) {
                            yield synth;
                        }
                        return;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        match serde_json::from_str::<ChatCompletionChunk>(data) {
                            Ok(chunk) => {
                                let Some(choice) = chunk.choices.first() else { continue; };

                                if let Some(content) = &choice.delta.content
                                    && !content.is_empty()
                                {
                                    yield content.clone();
                                }

                                if let Some(tool_calls) = &choice.delta.tool_calls {
                                    for tc in tool_calls {
                                        let entry = tool_accum.entry(tc.index).or_default();
                                        if let Some(func) = &tc.function {
                                            if let Some(name) = &func.name
                                                && !name.is_empty()
                                            {
                                                entry.0 = name.clone();
                                            }
                                            if let Some(args) = &func.arguments {
                                                entry.1.push_str(args);
                                            }
                                        }
                                    }
                                }

                                // A non-null finish_reason ends this turn; emit
                                // any tool calls the model accumulated.
                                if choice.finish_reason.is_some() {
                                    for synth in flush_streamed_tool_calls(&mut tool_accum) {
                                        yield synth;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse SSE data: {} ({})", data, e);
                            }
                        }
                    }
                }
            }

            // Stream closed without an explicit [DONE]: flush anything pending.
            for synth in flush_streamed_tool_calls(&mut tool_accum) {
                yield synth;
            }
        };

        Ok(Box::pin(output_stream))
    }

    /// Generate embeddings for a batch of inputs. Returns one vector per
    /// input, in input order.
    pub async fn embeddings(
        &self,
        host: &str,
        port: u16,
        model: &str,
        input: Vec<String>,
    ) -> Result<Vec<Vec<f32>>> {
        if !self.circuit_breaker.is_available() {
            tracing::warn!("vLLM circuit breaker is open");
            return Err(anyhow::anyhow!(
                "Circuit breaker open: vLLM temporarily unavailable"
            ));
        }

        let host = host
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let url = format!("http://{}:{}/v1/embeddings", host, port);

        let input_count = input.len();
        let request = EmbeddingsRequest {
            model: model.to_string(),
            input,
        };

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
            tracing::error!("vLLM embeddings returned error {}: {}", status, err_text);
            self.circuit_breaker.call_failed();
            anyhow::bail!("vLLM embeddings returned error {}: {}", status, err_text);
        }

        self.circuit_breaker.call_succeeded();
        let mut response: EmbeddingsResponse = res
            .json()
            .await
            .context("Failed to parse vLLM embeddings response")?;

        if response.data.len() != input_count {
            anyhow::bail!(
                "vLLM returned {} embeddings for {} inputs",
                response.data.len(),
                input_count
            );
        }

        response.data.sort_by_key(|d| d.index);
        Ok(response.data.into_iter().map(|d| d.embedding).collect())
    }
}

#[derive(Debug, Serialize)]
struct EmbeddingsRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    index: usize,
    embedding: Vec<f32>,
}
