//! The JSON the API speaks. Timestamps stay RFC 3339 text: the browser only shows them.

use quench_serde::{Deserialize, Serialize, Value};

/// The video memory of the machine, from `GET /api/v1/gpu/status`. A machine with no readable card answers all zeros and an
/// empty name.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Gpu {
    pub name: String,
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
}

impl Gpu {
    /// There is a card to talk about.
    #[must_use]
    pub fn present(&self) -> bool {
        !self.name.is_empty()
    }
}

/// What the person may do and what is switched on, from `GET /api/v1/vllm/capabilities`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub models_dashboard: bool,
    pub vllm_management: bool,
    pub can_launch: bool,
    pub can_stop: bool,
    pub can_delete_model: bool,
    /// Where instances run (`native`, `mock`, a Kubernetes namespace).
    pub namespace: String,
}

/// A context length. `ALL` (written `"ALL"` by the API, `0` here) is the model's own largest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Context(pub u32);

impl Context {
    /// The lengths the filter offers.
    pub const SIZES: [u32; 9] = [512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131_072];

    /// The number it stands for (`ALL` is the largest one).
    #[must_use]
    pub const fn as_usize(self) -> usize {
        if self.0 == 0 {
            131_072
        } else {
            self.0 as usize
        }
    }
}

impl std::fmt::Display for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 0 {
            f.write_str("ALL")
        } else {
            write!(f, "{}", self.0)
        }
    }
}

impl Serialize for Context {
    fn serialize(&self) -> Value {
        if self.0 == 0 {
            Value::String("ALL".into())
        } else {
            Value::UInt(u64::from(self.0))
        }
    }
}

impl Deserialize for Context {
    fn deserialize(value: &Value) -> quench_serde::Result<Self> {
        let text = match value {
            Value::String(s) => s.clone(),
            Value::Int(n) => n.to_string(),
            Value::UInt(n) => n.to_string(),
            Value::Float(n) => format!("{n:.0}"),
            _ => {
                return Err(quench_serde::Error::new(format!(
                    "invalid context type: {value:?}"
                )));
            }
        };
        if matches!(text.as_str(), "0" | "all" | "ALL") {
            return Ok(Self(0));
        }
        text.parse()
            .map(Self)
            .map_err(|_| quench_serde::Error::new(format!("invalid context value: {text}")))
    }
}

/// One what-if row of a model: the memory it would take at a quantisation and context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Estimate {
    pub quant: String,
    pub context: Context,
    pub weights_gb: f64,
    pub kv_gb: f64,
    pub total_gb: f64,
}

/// A model found on the drive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub source: String,
    pub name: String,
    pub path: String,
    pub architecture: Option<String>,
    pub vllm_supported: bool,
    pub quant: String,
    pub context: Context,
    pub layers: usize,
    pub hidden_size: usize,
    pub params_billion: f64,
    pub estimates: Vec<Estimate>,
}

/// The body of `POST /api/v1/models/list`: what the registry's filters ask for. `context` is text (`"0"` for all).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFilters {
    pub source: String,
    pub search: String,
    pub sort: String,
    pub quant: String,
    pub context: String,
    pub vllm_only: bool,
}

/// The body of `POST /api/v1/models/delete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteBody {
    pub path: String,
}

/// A vLLM instance, from `GET /api/v1/vllm/instances`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub namespace: String,
    pub model: String,
    pub host: String,
    pub port: u16,
    pub quantization: Option<String>,
    #[serde(default)]
    pub dtype: Option<String>,
    #[serde(default)]
    pub limit_mm_per_prompt: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,
    pub enable_tool_calling: bool,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub device: Option<String>,
    pub started_at: String,
    pub status: String,
    pub log_path: Option<String>,
    pub last_error: Option<String>,
}

/// How an instance's status is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Warn,
    Failed,
}

impl Instance {
    /// Only `cpu` (any case) runs on the CPU: anything else, including nothing, is a GPU launch.
    #[must_use]
    pub fn is_cpu(&self) -> bool {
        self.device
            .as_deref()
            .is_some_and(|d| d.trim().eq_ignore_ascii_case("cpu"))
    }

    /// A stop button makes sense until it has failed or is already going.
    #[must_use]
    pub fn stoppable(&self) -> bool {
        self.status != "failed" && self.status != "terminating"
    }

    #[must_use]
    pub fn tone(&self) -> Tone {
        match self.status.as_str() {
            "failed" | "Failed" | "completed" | "interrupted" => Tone::Failed,
            "starting" | "terminating" => Tone::Warn,
            _ => Tone::Ok,
        }
    }

    /// `host:port`.
    #[must_use]
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// `2026-10-29 10:00:00` from the RFC 3339 text.
    #[must_use]
    pub fn started(&self) -> String {
        self.started_at
            .chars()
            .take(19)
            .map(|c| if c == 'T' { ' ' } else { c })
            .collect()
    }

    /// The quantisation, `auto` when none was asked for.
    #[must_use]
    pub fn quant_text(&self) -> String {
        self.quantization
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .unwrap_or("auto")
            .to_string()
    }

    /// The GPU memory share (the default 0.90 when the instance did not say), as shown.
    #[must_use]
    pub fn gpu_util_text(&self) -> String {
        format!("{:.2}", self.gpu_memory_utilization.unwrap_or(0.9))
    }
}

/// The body of `POST /api/v1/vllm/instances`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchBody {
    pub model: String,
    pub host: String,
    pub port: u16,
    pub namespace: Option<String>,
    pub quantization: Option<String>,
    pub dtype: Option<String>,
    pub limit_mm_per_prompt: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f64>,
    pub enable_prefix_caching: bool,
    pub enable_tool_calling: bool,
    pub task: Option<String>,
    pub device: Option<String>,
}
