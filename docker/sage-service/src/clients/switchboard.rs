use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    base_url: String,
    username: String,
    password: String,
    http: reqwest::Client,
}

impl SwitchboardClient {
    pub fn new() -> Self {
        let base_url = envmnt::get_or("SWITCHBOARD_URL", "http://switchboard-service:8080");
        let username = envmnt::get_or_panic("SWITCHBOARD_TECH_USERNAME");
        let password = envmnt::get_or_panic("SWITCHBOARD_TECH_PASSWORD");
        let tls_verify = envmnt::get_or("SWITCHBOARD_TLS_VERIFY", "true")
            .parse::<bool>()
            .unwrap_or(true);

        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(!tls_verify)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            base_url,
            username,
            password,
            http,
        }
    }

    pub async fn get_vllm_instances(&self) -> Result<Vec<VllmInstance>> {
        let url = format!("{}/api/v1/vllm/instances", self.base_url);
        let res = self
            .http
            .get(&url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .context("Failed to connect to switchboard-service")?;

        if !res.status().is_success() {
            anyhow::bail!("Switchboard returned error: {}", res.status());
        }

        res.json::<Vec<VllmInstance>>()
            .await
            .context("Failed to parse vLLM instances")
    }
}
