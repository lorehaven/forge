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
    /// Multimodal input limit the instance was launched with (passed verbatim
    /// as `--limit-mm-per-prompt`, e.g. `{"image": 4}`); None = vLLM default
    /// (1 item per modality).
    #[serde(default)]
    pub limit_mm_per_prompt: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,
    pub enable_tool_calling: bool,
    /// vLLM task the instance was launched with (e.g. "embed"); None = generate.
    #[serde(default)]
    pub task: Option<String>,
    /// Execution device the instance was launched on (passed as `--device`,
    /// e.g. "cpu"). None / "gpu" / "auto" = vLLM's platform default (GPU),
    /// i.e. the pre-device-option behaviour.
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
    /// Multimodal input limit to launch with, passed verbatim as
    /// `--limit-mm-per-prompt` (e.g. `{"image": 4}` to allow 4 images per
    /// request on a vision model). None = vLLM default (1 per modality).
    #[serde(default)]
    pub limit_mm_per_prompt: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,
    #[serde(default)]
    pub enable_tool_calling: bool,
    /// vLLM task to launch with (e.g. "embed" for embedding models so
    /// /v1/embeddings is served). None = vLLM default. Translated into
    /// `--runner`/`--convert` flags at launch time (see [`task_launch_args`]).
    #[serde(default)]
    pub task: Option<String>,
    /// Execution device to launch on. None (the default) keeps the historical
    /// GPU behaviour: nothing is passed and vLLM auto-selects the platform
    /// accelerator. `"cpu"` launches with `--device cpu` and skips
    /// `--gpu-memory-utilization`; any other value is passed through verbatim
    /// as `--device <value>` (see [`device_launch_args`]).
    #[serde(default)]
    pub device: Option<String>,
}

/// Whether `device` selects CPU execution (case-insensitive `"cpu"`).
pub fn is_cpu_device(device: Option<&str>) -> bool {
    matches!(device, Some(d) if d.trim().eq_ignore_ascii_case("cpu"))
}

/// Translate a `device` value into vLLM CLI flags.
///
/// CPU execution is selected by the *runtime* - a CPU-only vLLM build in
/// native mode, the `vllm-openai-cpu` image in Kubernetes mode - never by a
/// flag: current vLLM repurposed `--device` for GPU device *ids* and rejects
/// `--device cpu` outright (`int("cpu")`). So `""` / `"gpu"` / `"auto"` /
/// `"default"` / `"cpu"` all yield nothing; only an explicit non-CPU
/// accelerator name (`"cuda"`, `"neuron"`, …) is passed as `--device <value>`.
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

/// GiB the CPU backend reserves for the KV cache, from `VLLM_CPU_KVCACHE_SPACE`
/// in the parent environment or a modest default. vLLM has no
/// `--gpu-memory-utilization` equivalent for CPU; this env var is the knob.
pub fn cpu_kvcache_space_gib() -> String {
    std::env::var("VLLM_CPU_KVCACHE_SPACE").unwrap_or_else(|_| "4".to_string())
}

/// Container image the Kubernetes engine runs for `device: "cpu"` launches,
/// from `VLLM_CPU_IMAGE` or the upstream default. Unlike the GPU path (which
/// runs vLLM from a host-mounted venv), the CPU image ships its own vLLM and
/// entrypoint, so a CPU launch needs no venv / ROCm / device mounts.
pub fn cpu_image() -> String {
    std::env::var("VLLM_CPU_IMAGE")
        .unwrap_or_else(|_| "vllm/vllm-openai-cpu:latest-x86_64".to_string())
}

/// Translate a task value (e.g. "embed", "generate") into vLLM CLI flags.
///
/// Current vLLM removed the `--task` flag in favour of `--runner`
/// ({auto,draft,generate,pooling}) and `--convert` ({auto,classify,embed,none}).
/// `--task embed` used to serve /v1/embeddings; the equivalent is now
/// `--runner pooling --convert embed`.
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

/// Recover a task value ("embed"/"generate") from a running vLLM instance's
/// CLI args, the inverse of [`task_launch_args`]. Used when reconstructing
/// instance state from live processes.
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
