use quench_client::ClientCredentialsClient;
use quench_core::Result;
use quench_core::time::Time;
use quench_serde::{Deserialize, Serialize};
use quench_starter::metrics::{RequestMetrics, TimedBlock};
use quench_starter::resilience::{CircuitBreaker, RetryConfig, retry_with_backoff};
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
    /// vLLM task the instance was launched with (e.g. "embed"); embedding instances serve
    /// /v1/embeddings only, so they must be excluded from the chat model selector.
    #[serde(default)]
    pub task: Option<String>,
    /// Execution device the instance was launched on (e.g. "cpu"); None = GPU.
    #[serde(default)]
    pub device: Option<String>,
    pub started_at: Time,
    pub status: String,
}

impl VllmInstance {
    /// Whether this instance can serve chat completions; embedding (pooling) instances yield a 404 if routed chat.
    #[must_use]
    pub fn is_chat_capable(&self) -> bool {
        !matches!(self.task.as_deref(), Some("embed" | "embedding"))
    }
}

#[derive(Clone)]
pub struct SwitchboardClient {
    client: ClientCredentialsClient,
    metrics: Arc<RequestMetrics>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl Default for SwitchboardClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SwitchboardClient {
    #[must_use]
    pub fn new() -> Self {
        let base_url =
            quench_core::env::get_or("SWITCHBOARD_URL", "http://switchboard-service:8080");
        let gatehouse_url = quench_core::env::get_or("GATEHOUSE_URL", "");
        let client_secret = quench_core::env::get_or("CLIENT_SECRET_SAGE_SWITCHBOARD", "");

        assert!(
            !(gatehouse_url.is_empty() || client_secret.is_empty()),
            "Missing required environment variables: GATEHOUSE_URL={}, \
             CLIENT_SECRET_SAGE_SWITCHBOARD={}",
            if gatehouse_url.is_empty() {
                "NOT SET"
            } else {
                "SET"
            },
            if client_secret.is_empty() {
                "NOT SET"
            } else {
                "SET"
            },
        );

        let tls_verify = quench_core::env::get_or("SWITCHBOARD_TLS_VERIFY", "true")
            .parse::<bool>()
            .unwrap_or(true);

        quench_log::info!(
            "Initializing SwitchboardClient with URL: {}, tls_verify: {}",
            base_url,
            tls_verify
        );

        let token_url = format!("{}/api/v1/token", gatehouse_url.trim_end_matches('/'));
        let client = ClientCredentialsClient::builder(&base_url)
            .token_url(&token_url)
            .client_id("sage-switchboard")
            .client_secret(&client_secret)
            .tls_verify(tls_verify)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            metrics: Arc::new(RequestMetrics::new()),
            circuit_breaker: Arc::new(CircuitBreaker::new(5, 3, 60)),
        }
    }

    #[must_use]
    pub fn metrics(&self) -> Arc<RequestMetrics> {
        self.metrics.clone()
    }

    pub async fn get_vllm_instances(&self) -> Result<Vec<VllmInstance>> {
        if !self.circuit_breaker.is_available() {
            quench_log::warn!("Switchboard circuit breaker is open");
            return Err(quench_core::err!(
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
                    .map_err(|e| format!("{e}"))
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
                quench_log::debug!("get_vllm_instances completed in {}ms", latency_ms);
            }
            Err(e) => {
                metrics.record_error(&e.clone());
                self.circuit_breaker.call_failed();
                quench_log::error!("get_vllm_instances failed: {}", e);
            }
        }

        result.map_err(|e| quench_core::err!("{e}"))
    }

    /// Request a graceful stop of a running vLLM instance (switchboard sends SIGTERM so it can
    /// drain and shut down cleanly). The endpoint returns an HTML fragment, so the body is ignored.
    pub async fn stop_instance(&self, id: &str) -> Result<()> {
        if !self.circuit_breaker.is_available() {
            quench_log::warn!("Switchboard circuit breaker is open");
            return Err(quench_core::err!(
                "Circuit breaker open: Switchboard temporarily unavailable"
            ));
        }

        let client = self.client.clone();
        let metrics = self.metrics.clone();
        let timer = TimedBlock::new();
        let path = format!("/api/v1/vllm/instances/{id}");

        let result = retry_with_backoff(
            || {
                let client = client.clone();
                let path = path.clone();
                async move {
                    client
                        .delete_expect_success(&path)
                        .await
                        .map_err(|e| format!("{e}"))
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
            Ok(()) => {
                let latency_ms = timer.elapsed_ms();
                metrics.record_success(latency_ms);
                self.circuit_breaker.call_succeeded();
                quench_log::info!("stop_instance({}) completed in {}ms", id, latency_ms);
            }
            Err(e) => {
                metrics.record_error(&e.clone());
                self.circuit_breaker.call_failed();
                quench_log::error!("stop_instance({}) failed: {}", id, e);
            }
        }

        result.map_err(|e| quench_core::err!("{e}"))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn launch_instance(
        &self,
        model: &str,
        gpu_memory_utilization: Option<f32>,
        max_model_len: Option<u32>,
        quantization: Option<&str>,
        dtype: Option<&str>,
        limit_mm_per_prompt: Option<&str>,
        max_num_seqs: Option<u32>,
        enable_tool_calling: bool,
        task: Option<&str>,
        device: Option<&str>,
    ) -> Result<VllmInstance> {
        if !self.circuit_breaker.is_available() {
            quench_log::warn!("Switchboard circuit breaker is open");
            return Err(quench_core::err!(
                "Circuit breaker open: Switchboard temporarily unavailable"
            ));
        }

        let req = quench_serde::json!({
            "model": model,
            "host": "0.0.0.0",
            "port": 8000,
            "namespace": null,
            "quantization": quantization,
            "dtype": dtype,
            "limit_mm_per_prompt": limit_mm_per_prompt,
            "max_num_seqs": max_num_seqs,
            "max_model_len": max_model_len,
            "gpu_memory_utilization": gpu_memory_utilization,
            "enable_prefix_caching": false,
            "enable_tool_calling": enable_tool_calling,
            "task": task,
            "device": device
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
                        .map_err(|e| format!("{e}"))
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
                quench_log::info!("launch_instance({}) completed in {}ms", model, latency_ms);
            }
            Err(e) => {
                metrics.record_error(&e.clone());
                self.circuit_breaker.call_failed();
                quench_log::error!("launch_instance({}) failed: {}", model, e);
            }
        }

        result.map_err(|e| quench_core::err!("{e}"))
    }
}

impl std::fmt::Debug for SwitchboardClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SwitchboardClient { .. }")
    }
}
