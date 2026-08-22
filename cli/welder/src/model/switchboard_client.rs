use anyhow::{Context, Result, anyhow, bail};
use quench_client::BearerAuthClient;
use serde::{Deserialize, Serialize};

/// A vLLM instance as reported by switchboard's `/api/v1/vllm/instances`.
///
/// Trimmed to the fields welder needs to route chat completions. `Serialize`
/// is only for tests to build mock response bodies with - welder never
/// sends this shape anywhere itself.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    client: BearerAuthClient,
}

impl std::fmt::Debug for SwitchboardClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SwitchboardClient").finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

impl SwitchboardClient {
    /// Logs in once against gatehouse with `WELDER_SWITCHBOARD_USERNAME` /
    /// `WELDER_SWITCHBOARD_PASSWORD` and uses the resulting bearer token for
    /// every switchboard call thereafter - switchboard's `Auth` middleware no
    /// longer accepts Basic auth directly.
    pub async fn new(base_url: &str, tls_verify: bool) -> Result<Self> {
        let username = std::env::var("WELDER_SWITCHBOARD_USERNAME")
            .context("WELDER_SWITCHBOARD_USERNAME must be set to use the switchboard backend")?;
        let password = std::env::var("WELDER_SWITCHBOARD_PASSWORD")
            .context("WELDER_SWITCHBOARD_PASSWORD must be set to use the switchboard backend")?;
        let gatehouse_url = std::env::var("GATEHOUSE_URL").context(
            "GATEHOUSE_URL must be set to use the switchboard backend - gatehouse is who \
             exchanges WELDER_SWITCHBOARD_USERNAME/PASSWORD for a token now",
        )?;

        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(!tls_verify)
            .build()
            .context("failed to build the gatehouse login client")?;
        let response = http
            .post(format!(
                "{}/api/v1/auth/login",
                gatehouse_url.trim_end_matches('/')
            ))
            .json(&serde_json::json!({ "username": username, "password": password }))
            .send()
            .await
            .context("failed to reach gatehouse to log in")?;
        if !response.status().is_success() {
            bail!("gatehouse rejected WELDER_SWITCHBOARD_USERNAME/PASSWORD");
        }
        let tokens: TokenResponse = response
            .json()
            .await
            .context("gatehouse's login response was not what was expected")?;

        let client = BearerAuthClient::with_tls_verify(base_url, &tokens.access_token, tls_verify)
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

    /// Bypasses the real `gatehouse` login flow `new` performs, for tests
    /// that only need to point `list_instances`/`launch_instance` at a
    /// `wiremock` server. The bearer token is never checked by anything
    /// other than a real gatehouse-issued middleware, which a mock server
    /// doesn't run.
    #[must_use]
    pub fn for_tests(base_url: &str) -> Self {
        let client = BearerAuthClient::with_tls_verify(base_url, "test-token", false)
            .expect("build a test switchboard client");
        Self { client }
    }
}
