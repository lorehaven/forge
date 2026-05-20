use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Workflow {
    pub root: Root,
    pub agent: Vec<AgentConfig>,
    pub vllm: Option<VllmConfig>,
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentVllmConfig {
    pub url: String,
    pub model_path: Option<String>,
    pub dtype: Option<String>,
    pub max_model_len: Option<usize>,
    pub gpu_memory_utilization: Option<f32>,
    pub tensor_parallel_size: Option<usize>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct VllmConfig {
    pub timeout_seconds: Option<u64>,
}
