pub mod caller;
pub mod switchboard_client;
pub mod vllm_client;

use crate::config::is_verbose;
use anyhow::Result;
use quench_cli::require::require_binary;
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use switchboard_client::{LaunchInstanceRequest, SwitchboardClient};
use tokio::time::sleep;

macro_rules! debug_log {
    ($($arg:tt)*) => {
        if is_verbose() {
            println!($($arg)*)
        }
    };
}

#[derive(Debug, Clone)]
pub struct ModelInstanceConfig {
    pub model: String,
    pub url: String,
    pub model_path: Option<String>,
    pub dtype: String,
    pub max_model_len: Option<usize>,
    pub gpu_memory_utilization: f32,
    pub tensor_parallel_size: usize,
}

#[derive(Debug, Clone)]
pub struct VllmConfig {
    pub timeout_seconds: u64,
}

impl Default for VllmConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 300, // 5 minutes
        }
    }
}

/// Manages multiple vLLM instances for different models
#[derive(Debug)]
pub struct ModelManager {
    instances: HashMap<(String, String), ModelInstanceConfig>,
    running_processes: Mutex<Vec<Child>>,
    vllm_config: VllmConfig,
}

impl ModelManager {
    #[must_use]
    pub fn new(vllm_config: VllmConfig) -> Self {
        Self {
            instances: HashMap::new(),
            running_processes: Mutex::new(Vec::new()),
            vllm_config,
        }
    }

    /// Register a model instance to be managed
    pub fn register(&mut self, config: ModelInstanceConfig) {
        self.instances
            .insert((config.model.clone(), config.url.clone()), config);
    }

    /// Initialize vLLM instances for all registered models
    pub async fn initialize(&mut self) -> Result<()> {
        let unique_instances: Vec<_> = self.instances.values().cloned().collect();

        if unique_instances.is_empty() {
            return Ok(());
        }

        println!(
            "[model-mgr] Initializing {} vLLM instance(s)...",
            unique_instances.len()
        );

        for config in unique_instances {
            let host_port = extract_host_port(&config.url)?;

            // Check if port is already in use
            if !Self::is_port_available(&host_port.0, host_port.1) {
                println!(
                    "[model-mgr] ⚠ {}:{} already in use, reusing existing instance",
                    host_port.0, host_port.1
                );
                continue;
            }

            println!("[model-mgr] Starting vLLM for: {}", config.model);
            println!("[model-mgr]   URL: {}", config.url);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let gpu_memory_percent = (config.gpu_memory_utilization * 100.0).round() as u32;
            println!("[model-mgr]   GPU Memory: {gpu_memory_percent}%");
            println!(
                "[model-mgr]   Tensor Parallel: {}",
                config.tensor_parallel_size
            );

            self.start_vllm_instance(&config)?;
            println!("[model-mgr] Waiting for server to be ready...");

            // Wait for server to be ready
            self.wait_for_server(&config.url).await?;
            println!("[model-mgr] ✓ Server ready on {}", config.url);
        }

        println!("[model-mgr] ✓ All vLLM instances initialized");
        Ok(())
    }

    /// Get the URL for a specific model
    pub fn get_url(&self, model: &str) -> Option<String> {
        self.instances
            .values()
            .find(|c| c.model == model)
            .map(|c| c.url.clone())
    }

    #[must_use]
    pub fn is_port_available(host: &str, port: u16) -> bool {
        std::net::TcpListener::bind((host, port)).is_ok()
    }

    fn start_vllm_instance(&self, config: &ModelInstanceConfig) -> Result<()> {
        require_binary(
            "vllm",
            "backend.kind = \"vllm\" needs the vllm CLI on PATH to serve local models",
        )?;
        let host_port = extract_host_port(&config.url)?;

        // Determine model argument: use model_path if provided, otherwise use model name
        let model_arg = config
            .model_path
            .clone()
            .unwrap_or_else(|| config.model.clone());

        debug_log!(
            "[model-mgr] Spawning: vllm serve {} --host {} --port {} --dtype {} --gpu-memory-utilization {} --tensor-parallel-size {}",
            model_arg,
            host_port.0,
            host_port.1,
            config.dtype,
            config.gpu_memory_utilization,
            config.tensor_parallel_size
        );

        let mut cmd = Command::new("vllm");
        cmd.arg("serve")
            .arg(&model_arg)
            .arg("--served-model-name")
            .arg(&config.model)
            .arg("--host")
            .arg(&host_port.0)
            .arg("--port")
            .arg(host_port.1.to_string())
            .arg("--dtype")
            .arg(&config.dtype)
            .arg("--gpu-memory-utilization")
            .arg(config.gpu_memory_utilization.to_string())
            .arg("--tensor-parallel-size")
            .arg(config.tensor_parallel_size.to_string());

        // Add optional max-model-len if specified
        if let Some(max_len) = config.max_model_len {
            cmd.arg("--max-model-len").arg(max_len.to_string());
        }

        // Suppress vLLM APIServer logs
        cmd.stdout(Stdio::null()).stderr(Stdio::null());

        let child = cmd.spawn()?;

        debug_log!("[model-mgr] Process spawned (PID: {:?})", child.id());

        self.running_processes.lock().unwrap().push(child);
        Ok(())
    }

    async fn wait_for_server(&self, url: &str) -> Result<()> {
        let host_port = extract_host_port(url)?;
        let addr = format!("{}:{}", host_port.0, host_port.1);
        let mut retries = 0;
        let max_retries = usize::try_from(self.vllm_config.timeout_seconds * 2)?; // 2 attempts per second (500ms each)

        while retries < max_retries {
            if std::net::TcpStream::connect(&addr).is_ok() {
                debug_log!("[vllm] Server connected after {}s", retries / 2);
                return Ok(());
            }
            retries += 1;

            // Log progress every 10 seconds (verbose only)
            if retries % 20 == 0 && is_verbose() {
                let elapsed = retries / 2;
                let remaining = self.vllm_config.timeout_seconds - (elapsed as u64);
                debug_log!(
                    "[vllm] Still waiting... ({}s elapsed, {}s remaining)",
                    elapsed,
                    remaining
                );
            }

            sleep(Duration::from_millis(500)).await;
        }

        Err(anyhow::anyhow!(
            "vLLM server on {} failed to start after {} seconds",
            addr,
            self.vllm_config.timeout_seconds
        ))
    }
}

impl Default for ModelManager {
    fn default() -> Self {
        Self::new(VllmConfig::default())
    }
}

pub fn extract_host_port(url: &str) -> Result<(String, u16)> {
    let parts: Vec<&str> = url.split(':').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!(
            "Invalid vLLM URL format: {url}. Expected 'host:port'",
        ));
    }
    let host = parts[0].to_string();
    let port = parts[1].parse::<u16>()?;
    Ok((host, port))
}

/// Launch hints for a model whose lifecycle is owned by switchboard rather
/// than welder. Used only if no matching instance is already running.
#[derive(Debug, Clone)]
pub struct SwitchboardModelConfig {
    pub model: String,
    pub quantization: Option<String>,
    pub dtype: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub limit_mm_per_prompt: Option<String>,
    pub task: Option<String>,
}

/// Resolves model names to running instance addresses via switchboard.
///
/// Launches instances that aren't already up. The counterpart of
/// `ModelManager` for the `switchboard` backend: same "register then
/// initialize" shape, but instance lifecycle lives in switchboard, not here.
#[derive(Debug)]
pub struct SwitchboardManager {
    client: SwitchboardClient,
    instances: HashMap<String, SwitchboardModelConfig>,
    resolved: HashMap<String, String>,
    timeout_seconds: u64,
}

impl SwitchboardManager {
    #[must_use]
    pub fn new(client: SwitchboardClient, timeout_seconds: u64) -> Self {
        Self {
            client,
            instances: HashMap::new(),
            resolved: HashMap::new(),
            timeout_seconds,
        }
    }

    /// Register a model to be resolved (and launched if necessary) against switchboard.
    pub fn register(&mut self, config: SwitchboardModelConfig) {
        self.instances.insert(config.model.clone(), config);
    }

    /// For each registered model: reuse a running instance if switchboard
    /// already has one, otherwise request a launch and wait for it to come up.
    pub async fn initialize(&mut self) -> Result<()> {
        let configs: Vec<_> = self.instances.values().cloned().collect();
        if configs.is_empty() {
            return Ok(());
        }

        println!(
            "[model-mgr] Resolving {} model(s) via switchboard...",
            configs.len()
        );

        for config in configs {
            if let Some(address) = self.find_running(&config.model).await? {
                println!(
                    "[model-mgr] ✓ {} already running on {address}",
                    config.model
                );
                self.resolved.insert(config.model.clone(), address);
                continue;
            }

            println!(
                "[model-mgr] Requesting switchboard launch: {}",
                config.model
            );
            self.client
                .launch_instance(LaunchInstanceRequest {
                    model: config.model.clone(),
                    host: "0.0.0.0".to_string(),
                    port: 8000,
                    namespace: None,
                    quantization: config.quantization.clone(),
                    dtype: config.dtype.clone(),
                    limit_mm_per_prompt: config.limit_mm_per_prompt.clone(),
                    max_model_len: config.max_model_len,
                    gpu_memory_utilization: config.gpu_memory_utilization,
                    enable_prefix_caching: false,
                    enable_tool_calling: false,
                    task: config.task.clone(),
                })
                .await?;

            println!(
                "[model-mgr] Waiting for {} to become ready...",
                config.model
            );
            let address = self.wait_for_running(&config.model).await?;
            println!("[model-mgr] ✓ {} ready on {address}", config.model);
            self.resolved.insert(config.model.clone(), address);
        }

        println!("[model-mgr] ✓ All switchboard-managed model(s) ready");
        Ok(())
    }

    /// Get the resolved `host:port` for a specific model.
    #[must_use]
    pub fn get_url(&self, model: &str) -> Option<String> {
        self.resolved.get(model).cloned()
    }

    pub async fn find_running(&self, model: &str) -> Result<Option<String>> {
        Ok(self
            .matching_instance(model)
            .await?
            .filter(switchboard_client::VllmInstance::is_running)
            .map(|i| i.address()))
    }

    async fn matching_instance(
        &self,
        model: &str,
    ) -> Result<Option<switchboard_client::VllmInstance>> {
        let instances = self.client.list_instances().await?;
        Ok(instances
            .into_iter()
            .find(|i| i.model == model && i.is_chat_capable()))
    }

    /// Number of *consecutive* failed `list_instances` calls tolerated while
    /// polling before giving up. A single dropped connection or a momentary
    /// 502 from switchboard shouldn't kill the whole wait loop, but a
    /// persistently broken connection still needs to surface as an error
    /// instead of silently spinning until `timeout_seconds` runs out.
    const MAX_CONSECUTIVE_POLL_ERRORS: u32 = 10;

    pub async fn wait_for_running(&self, model: &str) -> Result<String> {
        let max_retries = self.timeout_seconds * 2; // 2 attempts per second (500ms each)
        let mut consecutive_errors = 0u32;

        for attempt in 0..max_retries {
            match self.matching_instance(model).await {
                Ok(Some(instance)) => {
                    consecutive_errors = 0;
                    if instance.is_running() {
                        return Ok(instance.address());
                    }
                    if instance.is_failed() {
                        return Err(anyhow::anyhow!(
                            "switchboard instance for '{model}' failed to start"
                        ));
                    }
                }
                Ok(None) => {
                    consecutive_errors = 0;
                }
                Err(err) => {
                    consecutive_errors += 1;
                    if consecutive_errors >= Self::MAX_CONSECUTIVE_POLL_ERRORS {
                        return Err(err.context(format!(
                            "switchboard was unreachable for {consecutive_errors} consecutive poll attempts while waiting for '{model}'"
                        )));
                    }
                    debug_log!(
                        "[switchboard] poll for {model} failed ({consecutive_errors}/{}): {err}",
                        Self::MAX_CONSECUTIVE_POLL_ERRORS
                    );
                }
            }

            if attempt > 0 && attempt % 20 == 0 && is_verbose() {
                debug_log!(
                    "[switchboard] still waiting for {model}... ({}s elapsed)",
                    attempt / 2
                );
            }

            sleep(Duration::from_millis(500)).await;
        }

        Err(anyhow::anyhow!(
            "switchboard instance for '{model}' did not become ready after {}s",
            self.timeout_seconds
        ))
    }
}
