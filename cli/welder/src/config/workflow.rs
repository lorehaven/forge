use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Workflow {
    pub root: Root,
    pub agent: Vec<AgentConfig>,
    pub vllm: Option<VllmConfig>,
    pub switchboard: Option<SwitchboardConfig>,
    pub models: Option<HashMap<String, AgentVllmConfig>>,
}

#[derive(Debug, Deserialize)]
pub struct Root {
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub name: String,
    pub model: String,
    pub instruction: String,
    pub children: Option<Vec<String>>,
    pub tools: Option<Vec<String>>,
    pub max_tool_steps: Option<usize>,
    pub run_cmd_allowlist: Option<Vec<String>>,
    pub vllm: Option<AgentVllmConfig>,
    pub vllm_model: Option<String>,
    /// Sampling temperature for this agent's calls. Defaults to `0.7`.
    pub temperature: Option<f32>,
    /// Max tokens to generate per call for this agent. Defaults to `2048`.
    pub max_tokens: Option<usize>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentVllmConfig {
    /// Fixed `host:port` to spawn/reach the model at. Required for the local
    /// `vllm` backend, which owns the process; ignored by the `switchboard`
    /// backend, which discovers the instance's address dynamically.
    pub url: Option<String>,
    pub model_path: Option<String>,
    pub dtype: Option<String>,
    pub max_model_len: Option<usize>,
    pub gpu_memory_utilization: Option<f32>,
    pub tensor_parallel_size: Option<usize>,
    /// Launch hints passed to switchboard's `POST /api/v1/vllm/instances`
    /// when no matching instance is already running. Unused by the local
    /// `vllm` backend.
    pub quantization: Option<String>,
    pub task: Option<String>,
    pub limit_mm_per_prompt: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct VllmConfig {
    pub timeout_seconds: Option<u64>,
}

/// How long welder waits for switchboard to bring a launched instance to
/// `running` before giving up.
#[derive(Debug, Deserialize, Clone)]
pub struct SwitchboardConfig {
    pub timeout_seconds: Option<u64>,
}
