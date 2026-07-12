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
    /// vLLM task the instance was launched with (e.g. "embed"); None = generate.
    #[serde(default)]
    pub task: Option<String>,

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
    /// vLLM task to launch with (e.g. "embed" for embedding models so
    /// /v1/embeddings is served). None = vLLM default. Translated into
    /// `--runner`/`--convert` flags at launch time (see [`task_launch_args`]).
    #[serde(default)]
    pub task: Option<String>,
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
