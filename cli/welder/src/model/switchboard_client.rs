use anyhow::{Context, Result, anyhow};
use quench_client::BasicAuthClient;
use serde::{Deserialize, Serialize};

/// A vLLM instance as reported by switchboard's `/api/v1/vllm/instances`.
/// Trimmed to the fields welder needs to route chat completions.
#[derive(Debug, Clone, Deserialize)]
pub struct VllmInstance {
    pub model: String,
    pub host: String,
    pub port: u16,
    pub status: String,
    #[serde(default)]
    pub task: Option<String>,
}

impl VllmInstance {
    /// Pooling (embed/classify) instances only serve `/v1/embeddings`, not
    /// chat completions, so they can never satisfy an agent's request.
    #[must_use]
    pub fn is_chat_capable(&self) -> bool {
        !matches!(self.task.as_deref(), Some("embed" | "embedding"))
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.status == "running"
    }

    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.status == "failed"
    }

    #[must_use]
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchInstanceRequest {
    pub model: String,
    pub host: String,
    pub port: u16,
    pub namespace: Option<String>,
    pub quantization: Option<String>,
    pub dtype: Option<String>,
    pub limit_mm_per_prompt: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,
    pub enable_tool_calling: bool,
    pub task: Option<String>,
}

/// Talks to switchboard-service's vLLM management API so welder can discover
/// and launch model instances instead of managing `vllm serve` itself.
#[derive(Clone)]
pub struct SwitchboardClient {
    client: BasicAuthClient,
}

impl std::fmt::Debug for SwitchboardClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SwitchboardClient").finish_non_exhaustive()
    }
}

impl SwitchboardClient {
    /// Reads credentials from `WELDER_SWITCHBOARD_USERNAME` /
    /// `WELDER_SWITCHBOARD_PASSWORD`, matching how switchboard's `Auth`
    /// middleware validates HTTP Basic credentials against its own user db.
    pub fn new(base_url: &str, tls_verify: bool) -> Result<Self> {
        let username = std::env::var("WELDER_SWITCHBOARD_USERNAME").context(
            "WELDER_SWITCHBOARD_USERNAME must be set to use the switchboard backend",
        )?;
        let password = std::env::var("WELDER_SWITCHBOARD_PASSWORD").context(
            "WELDER_SWITCHBOARD_PASSWORD must be set to use the switchboard backend",
        )?;

        let client = BasicAuthClient::builder(base_url)
            .username(&username)
            .password(&password)
            .tls_verify(tls_verify)
            .build()
            .context("failed to build switchboard HTTP client")?;

        Ok(Self { client })
    }

    pub async fn list_instances(&self) -> Result<Vec<VllmInstance>> {
        self.client
            .get("/api/v1/vllm/instances")
            .await
            .map_err(|e| anyhow!("failed to list switchboard vLLM instances: {e}"))
    }

    pub async fn launch_instance(&self, req: LaunchInstanceRequest) -> Result<VllmInstance> {
        let model = req.model.clone();
        self.client
            .post("/api/v1/vllm/instances", &req)
            .await
            .map_err(|e| anyhow!("failed to launch switchboard vLLM instance for '{model}': {e}"))
    }
}
