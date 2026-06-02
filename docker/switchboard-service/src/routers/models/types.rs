use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
        write!(f, "{:?}", self)
    }
}

impl Quant {
    pub const HF_VALUES: [(Quant, f64); 7] = [
        (Quant::FP16, 2.0),
        (Quant::BF16, 2.0),
        (Quant::FP8, 1.0),
        (Quant::INT8, 1.0),
        (Quant::AWQ, 0.5),
        (Quant::GPTQ, 0.5),
        (Quant::Q80, 1.0),
    ];

    pub const GGUF_VALUES: [(Quant, f64); 8] = [
        (Quant::Q80, 1.0),
        (Quant::Q6K, 0.75),
        (Quant::Q5KM, 0.65),
        (Quant::Q50, 0.625),
        (Quant::Q4KM, 0.55),
        (Quant::Q40, 0.5),
        (Quant::Q3KM, 0.45),
        (Quant::Q2K, 0.35),
    ];

    pub fn bytes_per_weight(&self) -> f64 {
        match self {
            Quant::FP16 => 2.0,
            Quant::BF16 => 2.0,
            Quant::FP8 => 1.0,
            Quant::INT8 => 1.0,

            Quant::Q80 => 1.0,
            Quant::Q6K => 0.75,
            Quant::Q5KM => 0.65,
            Quant::Q50 => 0.625,
            Quant::Q4KM => 0.55,
            Quant::Q40 => 0.5,
            Quant::Q3KM => 0.45,
            Quant::Q2K => 0.35,

            Quant::AWQ => 0.5,
            Quant::GPTQ => 0.5,

            Quant::ALL => 0.0,
        }
    }

    pub fn rank(&self) -> i32 {
        match self {
            Quant::FP16 => 100,
            Quant::BF16 => 95,
            Quant::FP8 => 90,
            Quant::INT8 => 80,
            Quant::Q80 => 70,
            Quant::Q6K => 60,
            Quant::Q5KM => 50,
            Quant::Q50 => 45,
            Quant::Q4KM => 40,
            Quant::Q40 => 35,
            Quant::Q3KM => 30,
            Quant::Q2K => 20,
            Quant::AWQ => 55,
            Quant::GPTQ => 50,
            Quant::ALL => 0,
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
            Context::ALL => write!(f, "ALL"),
            Context::Size512 => write!(f, "512"),
            Context::Size1024 => write!(f, "1024"),
            Context::Size2048 => write!(f, "2048"),
            Context::Size4096 => write!(f, "4096"),
            Context::Size8192 => write!(f, "8192"),
            Context::Size16384 => write!(f, "16384"),
            Context::Size32768 => write!(f, "32768"),
            Context::Size65536 => write!(f, "65536"),
            Context::Size131072 => write!(f, "131072"),
        }
    }
}

impl<'de> Deserialize<'de> for Context {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        let s = match value {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            _ => {
                return Err(serde::de::Error::custom(format!(
                    "invalid context type: expected string or number, got {:?}",
                    value
                )));
            }
        };

        match s.as_str() {
            "0" | "all" | "ALL" => Ok(Context::ALL),

            "512" => Ok(Context::Size512),
            "1024" => Ok(Context::Size1024),
            "2048" => Ok(Context::Size2048),
            "4096" => Ok(Context::Size4096),
            "8192" => Ok(Context::Size8192),
            "16384" => Ok(Context::Size16384),
            "32768" => Ok(Context::Size32768),
            "65536" => Ok(Context::Size65536),
            "131072" => Ok(Context::Size131072),

            _ => Err(serde::de::Error::custom(format!(
                "invalid context value: {}",
                s
            ))),
        }
    }
}

impl Serialize for Context {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Context::ALL => serializer.serialize_str("ALL"),

            Context::Size512 => serializer.serialize_u32(512),
            Context::Size1024 => serializer.serialize_u32(1024),
            Context::Size2048 => serializer.serialize_u32(2048),
            Context::Size4096 => serializer.serialize_u32(4096),
            Context::Size8192 => serializer.serialize_u32(8192),
            Context::Size16384 => serializer.serialize_u32(16384),
            Context::Size32768 => serializer.serialize_u32(32768),
            Context::Size65536 => serializer.serialize_u32(65536),
            Context::Size131072 => serializer.serialize_u32(131072),
        }
    }
}

impl Context {
    pub const ALL_VALUES: [Context; 9] = [
        Context::Size512,
        Context::Size1024,
        Context::Size2048,
        Context::Size4096,
        Context::Size8192,
        Context::Size16384,
        Context::Size32768,
        Context::Size65536,
        Context::Size131072,
    ];

    pub fn as_usize(&self) -> usize {
        match self {
            Context::ALL => 131072,

            Context::Size512 => 512,
            Context::Size1024 => 1024,
            Context::Size2048 => 2048,
            Context::Size4096 => 4096,
            Context::Size8192 => 8192,
            Context::Size16384 => 16384,
            Context::Size32768 => 32768,
            Context::Size65536 => 65536,
            Context::Size131072 => 131072,
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
    pub context: Option<serde_json::Value>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_context_serialization_roundtrip() {
        let contexts = vec![
            (Context::Size512, json!(512)),
            (Context::Size1024, json!(1024)),
            (Context::ALL, json!("ALL")),
        ];

        for (ctx, expected_json) in contexts {
            // Test serialization
            let serialized = serde_json::to_value(ctx).unwrap();
            assert_eq!(serialized, expected_json);

            // Test deserialization from the serialized value
            let deserialized: Context = serde_json::from_value(serialized).unwrap();
            assert_eq!(deserialized, ctx);
        }
    }

    #[test]
    fn test_context_deserialization_from_string() {
        let json_str = json!("4096");
        let deserialized: Context = serde_json::from_value(json_str).unwrap();
        assert_eq!(deserialized, Context::Size4096);
    }

    #[test]
    fn test_model_deserialization_with_numeric_context() {
        let model_json = json!({
            "source": "HF",
            "name": "test-model",
            "path": "/path/to/model",
            "architecture": "LlamaForCausalLM",
            "vllm_supported": true,
            "quant": "FP16",
            "context": 4096,
            "layers": 32,
            "hidden_size": 4096,
            "params_billion": 7.0,
            "estimates": []
        });

        let model: Model = serde_json::from_value(model_json).unwrap();
        assert_eq!(model.context, Context::Size4096);
        assert_eq!(model.name, "test-model");
    }
}
