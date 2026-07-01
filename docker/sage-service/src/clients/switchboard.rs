use anyhow::Result;
use chrono::{DateTime, Utc};
use quench_client::BasicAuthClient;
use quench_starter::metrics::{RequestMetrics, TimedBlock};
use quench_starter::resilience::{RetryConfig, retry_with_backoff};
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
    pub started_at: DateTime<Utc>,
    pub status: String,
}

#[derive(Clone)]
pub struct SwitchboardClient {
    client: BasicAuthClient,
    metrics: Arc<RequestMetrics>,
}

impl SwitchboardClient {
    pub fn new() -> Self {
        let base_url = envmnt::get_or("SWITCHBOARD_URL", "http://switchboard-service:8080");
        let username = envmnt::get_or_panic("SWITCHBOARD_TECH_USERNAME");
        let password = envmnt::get_or_panic("SWITCHBOARD_TECH_PASSWORD");
        let tls_verify = envmnt::get_or("SWITCHBOARD_TLS_VERIFY", "true")
            .parse::<bool>()
            .unwrap_or(true);

        let client = BasicAuthClient::builder(&base_url)
            .username(&username)
            .password(&password)
            .tls_verify(tls_verify)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            metrics: Arc::new(RequestMetrics::new()),
        }
    }

    pub fn metrics(&self) -> Arc<RequestMetrics> {
        self.metrics.clone()
    }

    pub async fn get_vllm_instances(&self) -> Result<Vec<VllmInstance>> {
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
                tracing::debug!("get_vllm_instances completed in {}ms", latency_ms);
            }
            Err(e) => {
                metrics.record_error(&e.to_string());
                tracing::error!("get_vllm_instances failed: {}", e);
            }
        }

        result.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn launch_instance(
        &self,
        model: &str,
        gpu_memory_utilization: Option<f32>,
        max_model_len: Option<u32>,
        enable_tool_calling: bool,
    ) -> Result<VllmInstance> {
        let req = serde_json::json!({
            "model": model,
            "host": "0.0.0.0",
            "port": 8000,
            "namespace": null,
            "quantization": null,
            "max_model_len": max_model_len,
            "gpu_memory_utilization": gpu_memory_utilization,
            "enable_prefix_caching": false,
            "enable_tool_calling": enable_tool_calling
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
                tracing::info!("launch_instance({}) completed in {}ms", model, latency_ms);
            }
            Err(e) => {
                metrics.record_error(&e.to_string());
                tracing::error!("launch_instance({}) failed: {}", model, e);
            }
        }

        result.map_err(|e| anyhow::anyhow!("{}", e))
    }
}
