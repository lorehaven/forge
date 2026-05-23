pub mod caller;
pub mod vllm_client;

use crate::config::CONFIG;
use anyhow::Result;
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::sleep;

fn is_verbose() -> bool {
    // Check env var first (doesn't require CONFIG to be initialized)
    if let Ok(val) = std::env::var("WELDER_DEBUG")
        && (val.to_lowercase() == "true" || val == "1")
    {
        return true;
    }
    // Then check config (only after CONFIG is initialized)
    CONFIG.backend.debug
}

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

    fn is_port_available(host: &str, port: u16) -> bool {
        std::net::TcpListener::bind((host, port)).is_ok()
    }

    fn start_vllm_instance(&self, config: &ModelInstanceConfig) -> Result<()> {
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

fn extract_host_port(url: &str) -> Result<(String, u16)> {
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
