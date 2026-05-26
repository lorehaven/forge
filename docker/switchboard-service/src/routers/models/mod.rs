use actix_web::dev::HttpServiceFactory;
use actix_web::web::Json;
use actix_web::{HttpResponse, Responder, post, web};
use quench_srv::prelude::JwtConfig;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::Path;
use std::sync::LazyLock;
use utoipa::{OpenApi, ToSchema};

pub mod discovery;
pub mod store;

pub use discovery::{fetch_gguf_models, fetch_hf_models, fetch_models};
pub use store::{VLLM_SUPPORTED_ARCHITECTURES, get_store, init_model_store, warm_model_cache};

pub static HF_ROOTS: LazyLock<Vec<String>> =
    LazyLock::new(|| load_paths("HF_ROOTS", &["/mnt/dev/huggingface/hub"]));

pub static GGUF_ROOTS: LazyLock<Vec<String>> =
    LazyLock::new(|| load_paths("GGUF_ROOTS", &["/mnt/dev/quantized"]));

#[derive(Debug, Deserialize)]
pub struct VllmArchitecturesFile {
    pub architectures: Vec<String>,
}

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    paths(handle, delete_model),
    tags((name = "models", description = "Models endpoints"))
)]
pub struct ModelsApiDoc;

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/api/v1/models")
        .service(handle)
        .service(delete_model)
}

// ---------------------------------------------------------------------------
// Request type
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteModelRequest {
    pub path: String,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, ToSchema, PartialEq, Eq, Hash)]
pub enum ModelType {
    HF,
    GGUF,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, ToSchema, PartialEq, Eq, Hash)]
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
}

#[derive(Copy, Clone, Debug, ToSchema, PartialEq, Eq, Hash)]
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

impl<'de> Deserialize<'de> for Context {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        match value.as_str() {
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
                value
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct SearchQuery {
    pub r#type: ModelType,
    pub name: String,
    pub quant: Quant,
    pub context: Context,
}

// ---------------------------------------------------------------------------
// Response type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelEstimate {
    pub quant: Quant,
    pub context: Context,
    pub weights_gb: f64,
    pub kv_gb: f64,
    pub total_gb: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
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

// ---------------------------------------------------------------------------
// Fetch endpoint
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/list",
    operation_id = "fetch_available_models",
    tags = ["models"],
    request_body(
        content = SearchQuery,
        description = "Model search filters",
        content_type = "application/json"
    ),
    responses(
        (status = 200, description = "List of available models", body = Vec<Model>, content_type = "application/json"),
    )
)]
#[post("/list")]
async fn handle(body: Json<SearchQuery>) -> impl Responder {
    HttpResponse::Ok().json(fetch_models(body.r#type, &body.name, body.quant, body.context).await)
}

#[utoipa::path(
    post,
    path = "/delete",
    operation_id = "delete_model",
    tags = ["models"],
    request_body(
        content = DeleteModelRequest,
        description = "Model deletion request",
        content_type = "application/json"
    ),
    responses(
        (status = 200, description = "Model deleted successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin role required"),
        (status = 500, description = "Failed to delete model"),
    )
)]
#[post("/delete")]
async fn delete_model(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    body: Json<DeleteModelRequest>,
) -> impl Responder {
    if !config.auth_enabled {
        // If auth is disabled, allow deletion (development mode)
    } else {
        let cookie_name = format!("{}_ui_session", config.service_name);
        let Some(cookie) = req.cookie(&cookie_name) else {
            return HttpResponse::Unauthorized().finish();
        };

        match config.decode_claims(cookie.value()) {
            Ok(claims) => {
                if !claims.scope.contains("admin") {
                    return HttpResponse::Forbidden().finish();
                }
            }
            Err(_) => return HttpResponse::Unauthorized().finish(),
        }
    }

    let path = Path::new(&body.path);

    // Security check: ensure the path is within HF_ROOTS or GGUF_ROOTS
    let is_valid_hf = HF_ROOTS.iter().any(|root| path.starts_with(root));
    let is_valid_gguf = GGUF_ROOTS.iter().any(|root| path.starts_with(root));

    if !is_valid_hf && !is_valid_gguf {
        return HttpResponse::Forbidden().body("Invalid model path");
    }

    if !path.exists() {
        return HttpResponse::NotFound().body("Model not found on disk");
    }

    let res = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };

    match res {
        Ok(_) => {
            get_store().remove_model(&body.path).await;
            HttpResponse::Ok().finish()
        }
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_paths(env_key: &str, defaults: &[&str]) -> Vec<String> {
    std::env::var(env_key)
        .ok()
        .map(|v| {
            v.split(':')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string())
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| defaults.iter().map(|s| s.to_string()).collect())
}
