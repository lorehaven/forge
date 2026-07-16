use anyhow::Result;
use chrono::{DateTime, Utc};
use quench_client::BasicAuthClient;
use quench_starter::metrics::{RequestMetrics, TimedBlock};
use quench_starter::resilience::{CircuitBreaker, RetryConfig, retry_with_backoff};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VllmInstance {
    pub id: String,
    pub namespace: String,
    pub model: String,
    pub host: String,
    pub port: u16,
    pub quantization: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,
    /// vLLM task the instance was launched with (e.g. "embed" for embedding
    /// models). Embedding instances serve /v1/embeddings only, not chat
    /// completions, so they must be excluded from the chat model selector.
    #[serde(default)]
    pub task: Option<String>,
    pub started_at: DateTime<Utc>,
    pub status: String,
}

impl VllmInstance {
    /// Whether this instance can serve chat completions. Embedding
    /// (pooling) instances cannot — routing chat to them yields a 404.
    pub fn is_chat_capable(&self) -> bool {
        !matches!(self.task.as_deref(), Some("embed") | Some("embedding"))
    }
}

#[derive(Clone)]
pub struct SwitchboardClient {
    client: BasicAuthClient,
    metrics: Arc<RequestMetrics>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl SwitchboardClient {
    pub fn new() -> Self {
        let base_url = envmnt::get_or("SWITCHBOARD_URL", "http://switchboard-service:8080");
        let username = envmnt::get_or("SWITCHBOARD_TECH_USERNAME", "");
        let password = envmnt::get_or("SWITCHBOARD_TECH_PASSWORD", "");

        if username.is_empty() || password.is_empty() {
            tracing::error!(
                "SWITCHBOARD_TECH_USERNAME or SWITCHBOARD_TECH_PASSWORD not set! \
                 username empty: {}, password empty: {}",
                username.is_empty(),
                password.is_empty()
            );
            panic!(
                "Missing required environment variables: \
                 SWITCHBOARD_TECH_USERNAME={}, SWITCHBOARD_TECH_PASSWORD={}",
                if username.is_empty() {
                    "NOT SET"
                } else {
                    "SET"
                },
                if password.is_empty() {
                    "NOT SET"
                } else {
                    "SET"
                }
            );
        }

        let tls_verify = envmnt::get_or("SWITCHBOARD_TLS_VERIFY", "true")
            .parse::<bool>()
            .unwrap_or(true);

        tracing::info!(
            "Initializing SwitchboardClient with URL: {}, username: {}, tls_verify: {}",
            base_url,
            username,
            tls_verify
        );

        let client = BasicAuthClient::builder(&base_url)
            .username(&username)
            .password(&password)
            .tls_verify(tls_verify)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            metrics: Arc::new(RequestMetrics::new()),
            circuit_breaker: Arc::new(CircuitBreaker::new(5, 3, 60)),
        }
    }

    pub fn metrics(&self) -> Arc<RequestMetrics> {
        self.metrics.clone()
    }

    pub async fn get_vllm_instances(&self) -> Result<Vec<VllmInstance>> {
        if !self.circuit_breaker.is_available() {
            tracing::warn!("Switchboard circuit breaker is open");
            return Err(anyhow::anyhow!(
                "Circuit breaker open: Switchboard temporarily unavailable"
            ));
        }

        let client = self.client.clone();
        let metrics = self.metrics.clone();
        let timer = TimedBlock::new();

        let result = retry_with_backoff(
            || async {
                client
                    .get("/api/v1/vllm/instances")
                    .await
                    .map_err(|e| format!("{}", e))
            },
            RetryConfig {
                max_attempts: 3,
                initial_delay_ms: 100,
                max_delay_ms: 2000,
                backoff_multiplier: 2.0,
            },
        )
        .await;

        match &result {
            Ok(_) => {
                let latency_ms = timer.elapsed_ms();
                metrics.record_success(latency_ms);
                self.circuit_breaker.call_succeeded();
                tracing::debug!("get_vllm_instances completed in {}ms", latency_ms);
            }
            Err(e) => {
                metrics.record_error(&e.to_string());
                self.circuit_breaker.call_failed();
                tracing::error!("get_vllm_instances failed: {}", e);
            }
        }

        result.map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Request a graceful stop of a running vLLM instance. Switchboard sends
    /// the underlying process a SIGTERM so vLLM can drain and shut down
    /// cleanly. The endpoint returns an HTML fragment, so the body is ignored.
    pub async fn stop_instance(&self, id: &str) -> Result<()> {
        if !self.circuit_breaker.is_available() {
            tracing::warn!("Switchboard circuit breaker is open");
            return Err(anyhow::anyhow!(
                "Circuit breaker open: Switchboard temporarily unavailable"
            ));
        }

        let client = self.client.clone();
        let metrics = self.metrics.clone();
        let timer = TimedBlock::new();
        let path = format!("/api/v1/vllm/instances/{}", id);

        let result = retry_with_backoff(
            || {
                let client = client.clone();
                let path = path.clone();
                async move {
                    client
                        .delete_expect_success(&path)
                        .await
                        .map_err(|e| format!("{}", e))
                }
            },
            RetryConfig {
                max_attempts: 2,
                initial_delay_ms: 500,
                max_delay_ms: 3000,
                backoff_multiplier: 2.0,
            },
        )
        .await;

        match &result {
            Ok(_) => {
                let latency_ms = timer.elapsed_ms();
                metrics.record_success(latency_ms);
                self.circuit_breaker.call_succeeded();
                tracing::info!("stop_instance({}) completed in {}ms", id, latency_ms);
            }
            Err(e) => {
                metrics.record_error(&e.to_string());
                self.circuit_breaker.call_failed();
                tracing::error!("stop_instance({}) failed: {}", id, e);
            }
        }

        result.map_err(|e| anyhow::anyhow!("{}", e))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn launch_instance(
        &self,
        model: &str,
        gpu_memory_utilization: Option<f32>,
        max_model_len: Option<u32>,
        quantization: Option<&str>,
        dtype: Option<&str>,
        enable_tool_calling: bool,
        task: Option<&str>,
    ) -> Result<VllmInstance> {
        if !self.circuit_breaker.is_available() {
            tracing::warn!("Switchboard circuit breaker is open");
            return Err(anyhow::anyhow!(
                "Circuit breaker open: Switchboard temporarily unavailable"
            ));
        }

        let req = serde_json::json!({
            "model": model,
            "host": "0.0.0.0",
            "port": 8000,
            "namespace": null,
            "quantization": quantization,
            "dtype": dtype,
            "max_model_len": max_model_len,
            "gpu_memory_utilization": gpu_memory_utilization,
            "enable_prefix_caching": false,
            "enable_tool_calling": enable_tool_calling,
            "task": task
        });

        let client = self.client.clone();
        let metrics = self.metrics.clone();
        let timer = TimedBlock::new();

        let result = retry_with_backoff(
            || {
                let req = req.clone();
                let client = client.clone();
                async move {
                    client
                        .post("/api/v1/vllm/instances", &req)
                        .await
                        .map_err(|e| format!("{}", e))
                }
            },
            RetryConfig {
                max_attempts: 2,
                initial_delay_ms: 500,
                max_delay_ms: 3000,
                backoff_multiplier: 2.0,
            },
        )
        .await;

        match &result {
            Ok(_) => {
                let latency_ms = timer.elapsed_ms();
                metrics.record_success(latency_ms);
                self.circuit_breaker.call_succeeded();
                tracing::info!("launch_instance({}) completed in {}ms", model, latency_ms);
            }
            Err(e) => {
                metrics.record_error(&e.to_string());
                self.circuit_breaker.call_failed();
                tracing::error!("launch_instance({}) failed: {}", model, e);
            }
        }

        result.map_err(|e| anyhow::anyhow!("{}", e))
    }
}
