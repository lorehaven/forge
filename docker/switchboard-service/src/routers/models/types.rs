use quench_serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ModelType {
    HF,
    GGUF,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Quant {
    #[serde(alias = "ALL", alias = "all")]
    ALL,

    #[serde(alias = "FP16", alias = "fp16")]
    FP16,
    #[serde(alias = "BF16", alias = "bf16")]
    BF16,
    #[serde(alias = "FP8", alias = "fp8")]
    FP8,
    #[serde(alias = "INT8", alias = "int8")]
    INT8,

    #[serde(alias = "Q8_0", alias = "Q80")]
    Q80,
    #[serde(alias = "Q6_K", alias = "Q6K")]
    Q6K,
    #[serde(alias = "Q5_K_M", alias = "Q5KM")]
    Q5KM,
    #[serde(alias = "Q5_0", alias = "Q50")]
    Q50,
    #[serde(alias = "Q4_K_M", alias = "Q4KM")]
    Q4KM,
    #[serde(alias = "Q4_0", alias = "Q40")]
    Q40,
    #[serde(alias = "Q3_K_M", alias = "Q3KM")]
    Q3KM,
    #[serde(alias = "Q2_K", alias = "Q2K")]
    Q2K,

    #[serde(alias = "AWQ", alias = "awq")]
    AWQ,
    #[serde(alias = "GPTQ", alias = "gptq")]
    GPTQ,
}

impl std::fmt::Display for Quant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Quant {
    pub const HF_VALUES: [(Self, f64); 7] = [
        (Self::FP16, 2.0),
        (Self::BF16, 2.0),
        (Self::FP8, 1.0),
        (Self::INT8, 1.0),
        (Self::AWQ, 0.5),
        (Self::GPTQ, 0.5),
        (Self::Q80, 1.0),
    ];

    pub const GGUF_VALUES: [(Self, f64); 8] = [
        (Self::Q80, 1.0),
        (Self::Q6K, 0.75),
        (Self::Q5KM, 0.65),
        (Self::Q50, 0.625),
        (Self::Q4KM, 0.55),
        (Self::Q40, 0.5),
        (Self::Q3KM, 0.45),
        (Self::Q2K, 0.35),
    ];

    #[must_use]
    pub const fn bytes_per_weight(&self) -> f64 {
        match self {
            Self::FP16 | Self::BF16 => 2.0,
            Self::FP8 | Self::INT8 | Self::Q80 => 1.0,
            Self::Q6K => 0.75,
            Self::Q5KM => 0.65,
            Self::Q50 => 0.625,
            Self::Q4KM => 0.55,
            Self::Q40 | Self::AWQ | Self::GPTQ => 0.5,
            Self::Q3KM => 0.45,
            Self::Q2K => 0.35,

            Self::ALL => 0.0,
        }
    }

    #[must_use]
    pub const fn rank(&self) -> i32 {
        match self {
            Self::FP16 => 100,
            Self::BF16 => 95,
            Self::FP8 => 90,
            Self::INT8 => 80,
            Self::Q80 => 70,
            Self::Q6K => 60,
            Self::Q5KM | Self::GPTQ => 50,
            Self::Q50 => 45,
            Self::Q4KM => 40,
            Self::Q40 => 35,
            Self::Q3KM => 30,
            Self::Q2K => 20,
            Self::AWQ => 55,
            Self::ALL => 0,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Context {
    ALL,

    Size512,
    Size1024,
    Size2048,
    Size4096,
    Size8192,
    Size16384,
    Size32768,
    Size65536,
    Size131072,
}

impl std::fmt::Display for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ALL => write!(f, "ALL"),
            Self::Size512 => write!(f, "512"),
            Self::Size1024 => write!(f, "1024"),
            Self::Size2048 => write!(f, "2048"),
            Self::Size4096 => write!(f, "4096"),
            Self::Size8192 => write!(f, "8192"),
            Self::Size16384 => write!(f, "16384"),
            Self::Size32768 => write!(f, "32768"),
            Self::Size65536 => write!(f, "65536"),
            Self::Size131072 => write!(f, "131072"),
        }
    }
}

impl Deserialize for Context {
    fn deserialize(value: &quench_serde::Value) -> quench_serde::Result<Self> {
        let s = match value {
            quench_serde::Value::String(s) => s.clone(),
            quench_serde::Value::Int(n) => n.to_string(),
            quench_serde::Value::UInt(n) => n.to_string(),
            quench_serde::Value::Float(n) => n.to_string(),
            _ => {
                return Err(quench_serde::Error::new(format!(
                    "invalid context type: expected string or number, got {value:?}"
                )));
            }
        };

        match s.as_str() {
            "0" | "all" | "ALL" => Ok(Self::ALL),

            "512" => Ok(Self::Size512),
            "1024" => Ok(Self::Size1024),
            "2048" => Ok(Self::Size2048),
            "4096" => Ok(Self::Size4096),
            "8192" => Ok(Self::Size8192),
            "16384" => Ok(Self::Size16384),
            "32768" => Ok(Self::Size32768),
            "65536" => Ok(Self::Size65536),
            "131072" => Ok(Self::Size131072),

            _ => Err(quench_serde::Error::new(format!(
                "invalid context value: {s}"
            ))),
        }
    }
}

impl Serialize for Context {
    fn serialize(&self) -> quench_serde::Value {
        match self {
            Self::ALL => quench_serde::Value::String("ALL".to_string()),

            Self::Size512 => quench_serde::Value::UInt(512),
            Self::Size1024 => quench_serde::Value::UInt(1024),
            Self::Size2048 => quench_serde::Value::UInt(2048),
            Self::Size4096 => quench_serde::Value::UInt(4096),
            Self::Size8192 => quench_serde::Value::UInt(8192),
            Self::Size16384 => quench_serde::Value::UInt(16384),
            Self::Size32768 => quench_serde::Value::UInt(32768),
            Self::Size65536 => quench_serde::Value::UInt(65536),
            Self::Size131072 => quench_serde::Value::UInt(131_072),
        }
    }
}

impl Context {
    pub const ALL_VALUES: [Self; 9] = [
        Self::Size512,
        Self::Size1024,
        Self::Size2048,
        Self::Size4096,
        Self::Size8192,
        Self::Size16384,
        Self::Size32768,
        Self::Size65536,
        Self::Size131072,
    ];

    #[must_use]
    pub const fn as_usize(&self) -> usize {
        match self {
            Self::ALL | Self::Size131072 => 131_072,

            Self::Size512 => 512,
            Self::Size1024 => 1024,
            Self::Size2048 => 2048,
            Self::Size4096 => 4096,
            Self::Size8192 => 8192,
            Self::Size16384 => 16384,
            Self::Size32768 => 32768,
            Self::Size65536 => 65536,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ModelFilters {
    #[serde(alias = "type")]
    pub source: Option<String>,
    pub search: Option<String>,
    pub sort: Option<String>,
    pub quant: Option<String>,
    pub context: Option<quench_serde::Value>,
    pub vllm_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEstimate {
    pub quant: Quant,
    pub context: Context,
    pub weights_gb: f64,
    pub kv_gb: f64,
    pub total_gb: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    pub source: String,
    pub name: String,
    pub path: String,
    pub architecture: Option<String>,
    pub vllm_supported: bool,
    pub quant: Quant,
    pub context: Context,
    pub layers: usize,
    pub hidden_size: usize,
    pub params_billion: f64,
    pub estimates: Vec<ModelEstimate>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunningModel {
    pub id: String,
    pub model: String,
    pub endpoint: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteModelRequest {
    pub path: String,
}
