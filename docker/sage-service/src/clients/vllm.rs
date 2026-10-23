use quench_core::stream::Stream;
use quench_core::stream::StreamExt;
use quench_core::{Context, Result};
use quench_serde::{Deserialize, Serialize};
use quench_starter::metrics::RequestMetrics;
use quench_starter::resilience::CircuitBreaker;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: quench_serde::Value,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Image attachments as data URIs. When present, the message serializes as an `OpenAI`
    /// content-parts array so vision models receive the images alongside the text.
    #[serde(default)]
    pub images: Option<Vec<String>>,
}

/// Serialize to `OpenAI` chat format: plain string `content` for text-only messages, an array of `text`/`image_url` parts when images are attached.
impl Serialize for ChatMessage {
    fn serialize(&self) -> quench_serde::Value {
        let has_images = self.images.as_ref().is_some_and(|v| !v.is_empty());
        let mut message = quench_serde::Map::new();
        message.insert("role", quench_serde::to_value(&self.role));
        if has_images {
            let mut parts: Vec<quench_serde::Value> = Vec::new();
            if !self.content.is_empty() {
                parts.push(quench_serde::json!({"type": "text", "text": self.content}));
            }
            for url in self.images.iter().flatten() {
                parts.push(quench_serde::json!({"type": "image_url", "image_url": {"url": url}}));
            }
            message.insert("content", quench_serde::Value::Array(parts));
        } else {
            message.insert("content", quench_serde::to_value(&self.content));
        }
        if let Some(tool_calls) = &self.tool_calls {
            message.insert("tool_calls", quench_serde::to_value(tool_calls));
        }
        quench_serde::Value::Object(message)
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
    pub tools: Option<Vec<quench_serde::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    /// Forwarded to the model's chat template; used to switch off the thinking phase of
    /// Qwen3.5/Gemma 4 (no reasoning parser is configured, so it would leak into replies).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_template_kwargs: Option<quench_serde::Value>,
    /// `OpenAI` `response_format`; used with a JSON schema for constrained (guided) decoding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<quench_serde::Value>,
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

/// One entry of a streamed `tool_calls` delta, keyed by `index`; the first
/// chunk carries `function.name`, later ones append `function.arguments`.
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

/// Drains the accumulator into `<tool_call>` text lines the text-based
/// parser understands, so native function calling reuses that pipeline.
pub fn flush_streamed_tool_calls(
    accum: &mut std::collections::BTreeMap<usize, (String, String)>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (_index, (name, args)) in std::mem::take(accum) {
        if name.is_empty() {
            continue;
        }
        // Arguments arrive as a JSON string; embed verbatim, defaulting to an empty object when absent.
        let args = if args.trim().is_empty() {
            "{}".to_string()
        } else {
            args
        };
        let name_json = quench_serde::json::to_string(&name);
        out.push(format!(
            "<tool_call>{{\"name\": {name_json}, \"arguments\": {args}}}</tool_call>"
        ));
    }
    out
}

#[derive(Clone)]
pub struct VllmClient {
    http: quench_client::transport::Client,
    metrics: Arc<RequestMetrics>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl Default for VllmClient {
    fn default() -> Self {
        Self::new()
    }
}

/// The chat-completions request for a streamed turn. Low temperature, for precision.
fn streaming_request(
    model: &str,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u32>,
    tools: Option<Vec<quench_serde::Value>>,
) -> ChatCompletionRequest {
    ChatCompletionRequest {
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
        chat_template_kwargs: Some(quench_serde::json!({ "enable_thinking": false })),
        response_format: None,
    }
}

impl VllmClient {
    #[must_use]
    pub fn new() -> Self {
        let tls_verify = quench_core::env::get_or("VLLM_TLS_VERIFY", "true")
            .parse::<bool>()
            .unwrap_or(true);

        let http = quench_client::transport::Client::builder()
            .danger_accept_invalid_certs(!tls_verify)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            http,
            metrics: Arc::new(RequestMetrics::new()),
            circuit_breaker: Arc::new(CircuitBreaker::new(3, 2, 60)),
        }
    }

    #[must_use]
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
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send + Sync>>> {
        self.chat_stream_with_tools(host, port, model, messages, max_tokens, None)
            .await
    }

    /// POSTs the request; a refusal trips the circuit breaker and becomes the error.
    async fn send_streaming(
        &self,
        url: &str,
        request: &ChatCompletionRequest,
    ) -> Result<quench_client::transport::Response> {
        let res = self
            .http
            .post(url)
            .json(request)
            .send_streaming()
            .await
            .with_context(|| format!("Failed to connect to vLLM instance at {url}"))?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            quench_log::error!("vLLM returned error {}: {}", status, err_text);
            self.circuit_breaker.call_failed();
            quench_core::bail!("vLLM returned error {status}: {err_text}");
        }

        Ok(res)
    }

    pub async fn chat_stream_with_tools(
        &self,
        host: &str,
        port: u16,
        model: &str,
        messages: Vec<ChatMessage>,
        max_tokens: Option<u32>,
        tools: Option<Vec<quench_serde::Value>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send + Sync>>> {
        if !self.circuit_breaker.is_available() {
            quench_log::warn!("vLLM circuit breaker is open");
            return Err(quench_core::err!(
                "Circuit breaker open: vLLM temporarily unavailable"
            ));
        }

        let host = host
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let url = format!("http://{host}:{port}/v1/chat/completions");

        let request = streaming_request(model, messages, max_tokens, tools);

        quench_log::info!("Connecting to vLLM at {url} (max_tokens: {max_tokens:?})");

        let res = self.send_streaming(&url, &request).await?;

        self.circuit_breaker.call_succeeded();
        let mut stream = res.bytes_stream();
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<String>>(32);

        // The reader runs as a task and feeds a channel: a generator would hold the response and the parser
        // state across every await, which a plain stream of the receiver does not.
        tokio::spawn(async move {
            let mut buffer = Vec::new();
            // Accumulates native (OpenAI) streamed tool calls by index until the model signals completion, then re-emits them as <tool_call> text.
            let mut tool_accum: std::collections::BTreeMap<usize, (String, String)> =
                std::collections::BTreeMap::new();

            while let Some(chunk_res) = stream.next().await {
                let chunk = match chunk_res {
                    Ok(chunk) => chunk,
                    Err(e) => {
                        let _ = tx
                            .send(Err(quench_core::err!(
                                "Error reading from vLLM stream: {e}"
                            )))
                            .await;
                        return;
                    }
                };
                buffer.extend_from_slice(&chunk);

                while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                    let line_bytes = buffer.drain(..=pos).collect::<Vec<u8>>();
                    let line_str = String::from_utf8_lossy(&line_bytes);
                    let line = line_str.trim();

                    if line.is_empty() {
                        continue;
                    }
                    if line == "data: [DONE]" {
                        for synth in flush_streamed_tool_calls(&mut tool_accum) {
                            if tx.send(Ok(synth)).await.is_err() {
                                return;
                            }
                        }
                        return;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        match quench_serde::json::from_str::<ChatCompletionChunk>(data) {
                            Ok(chunk) => {
                                let Some(choice) = chunk.choices.first() else {
                                    continue;
                                };

                                if let Some(content) = &choice.delta.content
                                    && !content.is_empty()
                                    && tx.send(Ok(content.clone())).await.is_err()
                                {
                                    return;
                                }

                                if let Some(tool_calls) = &choice.delta.tool_calls {
                                    for tc in tool_calls {
                                        let entry = tool_accum.entry(tc.index).or_default();
                                        if let Some(func) = &tc.function {
                                            if let Some(name) = &func.name
                                                && !name.is_empty()
                                            {
                                                entry.0.clone_from(name);
                                            }
                                            if let Some(args) = &func.arguments {
                                                entry.1.push_str(args);
                                            }
                                        }
                                    }
                                }

                                // A non-null finish_reason ends this turn; emit any accumulated tool calls.
                                if choice.finish_reason.is_some() {
                                    for synth in flush_streamed_tool_calls(&mut tool_accum) {
                                        if tx.send(Ok(synth)).await.is_err() {
                                            return;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                quench_log::warn!("Failed to parse SSE data: {} ({})", data, e);
                            }
                        }
                    }
                }
            }

            // Stream closed without an explicit [DONE]: flush anything pending.
            for synth in flush_streamed_tool_calls(&mut tool_accum) {
                if tx.send(Ok(synth)).await.is_err() {
                    return;
                }
            }
        });

        let output_stream = quench_http::stream::receiver_stream(rx);

        Ok(Box::pin(output_stream))
    }

    /// One non-streaming completion whose output is constrained to `schema` via vLLM guided
    /// decoding; returns the raw JSON text. Used for small structured steps (planning, grading).
    #[allow(clippy::too_many_arguments)]
    pub async fn chat_json(
        &self,
        host: &str,
        port: u16,
        model: &str,
        messages: Vec<ChatMessage>,
        schema_name: &str,
        schema: quench_serde::Value,
        max_tokens: u32,
    ) -> Result<String> {
        if !self.circuit_breaker.is_available() {
            return Err(quench_core::err!(
                "Circuit breaker open: vLLM temporarily unavailable"
            ));
        }

        let host = host
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let url = format!("http://{host}:{port}/v1/chat/completions");

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages,
            stream: false,
            temperature: Some(0.0),
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
            max_tokens: Some(max_tokens),
            tools: None,
            tool_choice: None,
            chat_template_kwargs: Some(quench_serde::json!({ "enable_thinking": false })),
            response_format: Some(quench_serde::json!({
                "type": "json_schema",
                "json_schema": { "name": schema_name, "schema": schema, "strict": true }
            })),
        };

        let res = self
            .http
            .post(&url)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("Failed to connect to vLLM instance at {url}"))?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            self.circuit_breaker.call_failed();
            quench_core::bail!("vLLM returned error {status}: {err_text}");
        }

        self.circuit_breaker.call_succeeded();
        let body: quench_serde::Value = res
            .json()
            .await
            .context("Failed to parse vLLM completion response")?;
        body["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_string)
            .context("vLLM completion had no message content")
    }

    /// Generate embeddings for a batch of inputs; returns one vector per input, in input order.
    pub async fn embeddings(
        &self,
        host: &str,
        port: u16,
        model: &str,
        input: Vec<String>,
    ) -> Result<Vec<Vec<f32>>> {
        if !self.circuit_breaker.is_available() {
            quench_log::warn!("vLLM circuit breaker is open");
            return Err(quench_core::err!(
                "Circuit breaker open: vLLM temporarily unavailable"
            ));
        }

        let host = host
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let url = format!("http://{host}:{port}/v1/embeddings");

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
            .with_context(|| format!("Failed to connect to vLLM instance at {url}"))?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            quench_log::error!("vLLM embeddings returned error {}: {}", status, err_text);
            self.circuit_breaker.call_failed();
            quench_core::bail!("vLLM embeddings returned error {status}: {err_text}");
        }

        self.circuit_breaker.call_succeeded();
        let mut response: EmbeddingsResponse = res
            .json()
            .await
            .context("Failed to parse vLLM embeddings response")?;

        if response.data.len() != input_count {
            quench_core::bail!(
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

impl std::fmt::Debug for VllmClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("VllmClient { .. }")
    }
}
