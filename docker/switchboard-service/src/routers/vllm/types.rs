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
    pub enable_tool_calling: bool,

    pub started_at: DateTime<Utc>,

    pub status: String,
    pub log_path: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LaunchRequest {
    pub model: String,
    pub host: String,
    pub port: u16,
    pub namespace: Option<String>,
    pub quantization: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,
    #[serde(default)]
    pub enable_tool_calling: bool,
}
