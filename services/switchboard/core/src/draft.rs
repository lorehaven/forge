//! Forms' text, checked into the body the API wants.

use crate::model::{LaunchBody, ModelFilters};

/// Why the launch form is not ready to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    ModelRequired,
}

impl DraftError {
    /// The text for a person, in English: operational, not run through the translations.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::ModelRequired => "choose a model to launch",
        }
    }
}

/// The sort orders of the registry, as the API names them.
pub const SORTS: [&str; 6] = [
    "name_asc",
    "name_desc",
    "params_asc",
    "params_desc",
    "vram_asc",
    "vram_desc",
];

/// The quantisations the filter offers per source (the API reads these spellings).
pub const HF_QUANTS: [&str; 6] = ["FP16", "BF16", "FP8", "INT8", "AWQ", "GPTQ"];
pub const GGUF_QUANTS: [&str; 8] = [
    "Q8_0", "Q6_K", "Q5_K_M", "Q5_0", "Q4_K_M", "Q4_0", "Q3_K_M", "Q2_K",
];

/// What the launch form offers.
pub const LAUNCH_QUANTS: [&str; 6] = [
    "awq",
    "gptq",
    "awq_marlin",
    "gptq_marlin",
    "fp8",
    "bitsandbytes",
];
pub const DTYPES: [&str; 3] = ["float16", "bfloat16", "float32"];
/// `(value, label)`: GPU is the empty value.
pub const DEVICES: [(&str, &str); 2] = [("", "GPU"), ("cpu", "CPU")];
pub const TASKS: [(&str, &str); 3] = [("", "auto"), ("generate", "generate"), ("embed", "embed")];

/// The registry's filters, as the form holds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterDraft {
    pub source: String,
    pub search: String,
    pub sort: String,
    /// `ALL` or one of the source's quantisations.
    pub quant: String,
    /// `0` for all.
    pub context: String,
    pub vllm_only: bool,
}

impl Default for FilterDraft {
    fn default() -> Self {
        Self {
            source: "hf".into(),
            search: String::new(),
            sort: "name_asc".into(),
            quant: "ALL".into(),
            context: "0".into(),
            vllm_only: false,
        }
    }
}

impl FilterDraft {
    /// The quantisations of the chosen source.
    #[must_use]
    pub fn quants(&self) -> &'static [&'static str] {
        if self.source == "gguf" {
            &GGUF_QUANTS
        } else {
            &HF_QUANTS
        }
    }

    #[must_use]
    pub fn body(&self) -> ModelFilters {
        ModelFilters {
            source: self.source.clone(),
            search: self.search.trim().to_string(),
            sort: self.sort.clone(),
            quant: self.quant.clone(),
            context: self.context.clone(),
            vllm_only: self.vllm_only,
        }
    }
}

/// Text to the optional field the API wants: `None` when there is only whitespace.
fn optional(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// A number the way the old form read it: blank or unreadable is "not given".
fn number<T: std::str::FromStr>(text: &str) -> Option<T> {
    text.trim().parse().ok()
}

/// The launch form, as it is typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchDraft {
    pub model: String,
    pub host: String,
    pub port: String,
    pub namespace: String,
    pub quantization: String,
    pub dtype: String,
    pub device: String,
    pub max_model_len: String,
    pub limit_mm: String,
    pub gpu_util: String,
    pub prefix_caching: bool,
    pub task: String,
    pub tool_calling: bool,
}

impl LaunchDraft {
    /// A fresh form: the defaults of the old one (`0.0.0.0:8000`, the engine's namespace).
    #[must_use]
    pub fn new(namespace: &str) -> Self {
        Self {
            model: String::new(),
            host: "0.0.0.0".into(),
            port: "8000".into(),
            namespace: namespace.into(),
            quantization: String::new(),
            dtype: String::new(),
            device: String::new(),
            max_model_len: String::new(),
            limit_mm: String::new(),
            gpu_util: "0.90".into(),
            prefix_caching: false,
            task: String::new(),
            tool_calling: false,
        }
    }

    #[must_use]
    pub fn is_cpu(&self) -> bool {
        self.device.trim().eq_ignore_ascii_case("cpu")
    }

    /// The length asked for, when it is a number.
    #[must_use]
    pub fn max_len(&self) -> Option<u32> {
        number(&self.max_model_len)
    }

    /// The memory share typed, when it is a number.
    #[must_use]
    pub fn typed_gpu_util(&self) -> Option<f32> {
        number(&self.gpu_util)
    }

    /// The body to send. As the old form did: a port or length that does not read is left to the defaults; a CPU launch
    /// carries no memory share.
    ///
    /// # Errors
    /// [`DraftError::ModelRequired`] when no model is chosen.
    pub fn to_body(&self) -> Result<LaunchBody, DraftError> {
        if self.model.trim().is_empty() {
            return Err(DraftError::ModelRequired);
        }
        let device = optional(&self.device);
        let cpu = self.is_cpu();
        Ok(LaunchBody {
            model: self.model.trim().to_string(),
            host: optional(&self.host).unwrap_or_else(|| "0.0.0.0".into()),
            port: number(&self.port).unwrap_or(8000),
            namespace: optional(&self.namespace),
            quantization: optional(&self.quantization),
            dtype: optional(&self.dtype),
            limit_mm_per_prompt: optional(&self.limit_mm),
            max_model_len: number(&self.max_model_len),
            gpu_memory_utilization: if cpu {
                None
            } else {
                Some(number(&self.gpu_util).unwrap_or(0.90))
            },
            enable_prefix_caching: self.prefix_caching,
            enable_tool_calling: self.tool_calling,
            task: optional(&self.task),
            device,
        })
    }
}
