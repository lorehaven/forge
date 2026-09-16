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
    /// vLLM weight/activation dtype the instance was launched with (passed as
    /// `--dtype`, e.g. "float16"); None = vLLM default ("auto", usually bfloat16).
    #[serde(default)]
    pub dtype: Option<String>,
    /// Multimodal limit (`--limit-mm-per-prompt`); None = vLLM default (1/modality).
    #[serde(default)]
    pub limit_mm_per_prompt: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,
    pub enable_tool_calling: bool,
    /// vLLM task the instance was launched with (e.g. "embed"); None = generate.
    #[serde(default)]
    pub task: Option<String>,
    /// Execution device (`--device`); None/"gpu"/"auto" = vLLM's GPU default.
    #[serde(default)]
    pub device: Option<String>,

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
    /// vLLM dtype to launch with (passed as `--dtype`, e.g. "float16" for GPUs
    /// or models that misbehave with the default bfloat16). None = vLLM "auto".
    #[serde(default)]
    pub dtype: Option<String>,
    /// Multimodal limit (`--limit-mm-per-prompt`, e.g. `{"image": 4}`);
    /// None = vLLM default (1/modality).
    #[serde(default)]
    pub limit_mm_per_prompt: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,
    #[serde(default)]
    pub enable_tool_calling: bool,
    /// vLLM task (e.g. "embed"); see [`task_launch_args`] for the CLI mapping.
    #[serde(default)]
    pub task: Option<String>,
    /// Execution device; None keeps GPU auto-select, `"cpu"` skips
    /// `--gpu-memory-utilization` - see [`device_launch_args`].
    #[serde(default)]
    pub device: Option<String>,
}

/// Whether `device` selects CPU execution (case-insensitive `"cpu"`).
pub fn is_cpu_device(device: Option<&str>) -> bool {
    matches!(device, Some(d) if d.trim().eq_ignore_ascii_case("cpu"))
}

/// CPU is selected by the runtime, not a flag - vLLM's `--device` rejects
/// `"cpu"`, so only a real accelerator name is passed through.
pub fn device_launch_args(device: &str) -> Vec<String> {
    match device.trim().to_lowercase().as_str() {
        "" | "gpu" | "auto" | "default" | "cpu" => vec![],
        other => vec!["--device".to_string(), other.to_string()],
    }
}

/// Recover a `device` value from a running instance's CLI args, the inverse of
/// [`device_launch_args`]. `None` when no `--device` flag is present.
pub fn device_from_args(parts: &[String]) -> Option<String> {
    parts
        .iter()
        .position(|p| p == "--device")
        .and_then(|i| parts.get(i + 1))
        .map(|v| v.to_string())
}

/// GiB the CPU backend reserves for KV cache - `VLLM_CPU_KVCACHE_SPACE` is
/// the CPU equivalent of `--gpu-memory-utilization`.
pub fn cpu_kvcache_space_gib() -> String {
    std::env::var("VLLM_CPU_KVCACHE_SPACE").unwrap_or_else(|_| "4".to_string())
}

/// CPU launch image - ships its own vLLM/entrypoint, so it needs no venv,
/// ROCm, or device mounts unlike the GPU path.
pub fn cpu_image() -> String {
    std::env::var("VLLM_CPU_IMAGE")
        .unwrap_or_else(|_| "vllm/vllm-openai-cpu:latest-x86_64".to_string())
}

/// Translate a task value into vLLM CLI flags - `--task` is gone in current
/// vLLM, replaced by `--runner`/`--convert` (e.g. embed = pooling+embed).
pub fn task_launch_args(task: &str) -> Vec<String> {
    match task {
        "embed" | "embedding" => vec![
            "--runner".to_string(),
            "pooling".to_string(),
            "--convert".to_string(),
            "embed".to_string(),
        ],
        "classify" => vec![
            "--runner".to_string(),
            "pooling".to_string(),
            "--convert".to_string(),
            "classify".to_string(),
        ],
        "generate" => vec!["--runner".to_string(), "generate".to_string()],
        // Pass anything else straight through as a --runner value
        // (e.g. "auto", "draft", "pooling").
        other => vec!["--runner".to_string(), other.to_string()],
    }
}

/// Recover a task value from CLI args, the inverse of [`task_launch_args`].
pub fn task_from_args(parts: &[String]) -> Option<String> {
    let flag_value = |flag: &str| {
        parts
            .iter()
            .position(|p| p == flag)
            .and_then(|i| parts.get(i + 1))
            .map(|v| v.to_string())
    };
    // Prefer --convert (embed/classify), then fall back to --runner, then the
    // legacy --task flag for instances launched by older switchboard builds.
    match flag_value("--convert").as_deref() {
        Some("embed") => return Some("embed".to_string()),
        Some("classify") => return Some("classify".to_string()),
        _ => {}
    }
    match flag_value("--runner").as_deref() {
        Some("pooling") => return Some("embed".to_string()),
        Some("generate") => return Some("generate".to_string()),
        _ => {}
    }
    flag_value("--task")
}
