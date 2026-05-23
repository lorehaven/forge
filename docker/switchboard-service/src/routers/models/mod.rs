use actix_web::dev::HttpServiceFactory;
use actix_web::web::Json;
use actix_web::{HttpResponse, Responder, post, web};
use gguf::GGUFMetadataValue;
use quench_srv::prelude::jwt::JwtConfig;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{LazyLock, RwLock};
use utoipa::{OpenApi, ToSchema};
use walkdir::WalkDir;

pub static HF_ROOTS: LazyLock<Vec<String>> =
    LazyLock::new(|| load_paths("HF_ROOTS", &["/mnt/dev/huggingface/hub"]));

pub static GGUF_ROOTS: LazyLock<Vec<String>> =
    LazyLock::new(|| load_paths("GGUF_ROOTS", &["/mnt/dev/quantized"]));

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    paths(handle),
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

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ModelEstimate {
    pub quant: Quant,
    pub context: Context,
    pub weights_gb: f64,
    pub kv_gb: f64,
    pub total_gb: f64,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Model {
    pub source: String,
    pub name: String,
    pub path: String,
    pub quant: Quant,
    pub context: Context,
    pub layers: usize,
    pub hidden_size: usize,
    pub params_billion: f64,
    pub estimates: Vec<ModelEstimate>,
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

static MODEL_CACHE: LazyLock<RwLock<HashMap<String, Model>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

fn get_cached_model(path: &str) -> Option<Model> {
    MODEL_CACHE.read().unwrap().get(path).cloned()
}

fn insert_cached_model(model: &Model) {
    MODEL_CACHE
        .write()
        .unwrap()
        .insert(model.path.clone(), model.clone());
}

pub fn warm_model_cache() {
    fetch_hf_models();
    fetch_gguf_models();
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
    HttpResponse::Ok().json(fetch_models(
        body.r#type,
        &body.name,
        body.quant,
        body.context,
    ))
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
            MODEL_CACHE.write().unwrap().remove(&body.path);
            HttpResponse::Ok().finish()
        }
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

fn fetch_models(model_type: ModelType, name: &str, quant: Quant, context: Context) -> Vec<Model> {
    let models = match model_type {
        ModelType::HF => fetch_hf_models(),
        ModelType::GGUF => fetch_gguf_models(),
    };

    models
        .into_iter()
        .filter(|model| name.is_empty() || model.name.to_lowercase().contains(&name.to_lowercase()))
        .filter(|model| quant == Quant::ALL || model.quant == quant)
        .filter(|model| context == Context::ALL || model.context == context)
        .collect()
}

fn fetch_hf_models() -> Vec<Model> {
    let mut models = vec![];
    let mut seen = HashSet::new();

    for root in HF_ROOTS.iter() {
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if entry.file_name() != "config.json" {
                continue;
            }

            let config_path = entry.path();
            let model_dir = match config_path.ancestors().nth(3) {
                Some(p) => p,
                None => continue,
            };

            // reach cache first
            let cache_key = model_dir.to_string_lossy().to_string();
            if let Some(model) = get_cached_model(&cache_key) {
                models.push(model);
                continue;
            }

            let name = normalize_hf_name(model_dir);
            if !seen.insert(name.clone()) {
                continue;
            }

            let content = match std::fs::read_to_string(config_path) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let json: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let hidden_size = json["hidden_size"].as_u64().unwrap_or(4096) as usize;
            let layers = json["num_hidden_layers"].as_u64().unwrap_or(32) as usize;
            let vocab_size = json["vocab_size"].as_u64().unwrap_or(32000) as f64;
            let max_position_embeddings = json["max_position_embeddings"].as_u64().unwrap_or(4096);
            let torch_dtype = json["torch_dtype"].as_str().unwrap_or("float16");

            let quant = infer_hf_quant(torch_dtype);
            let context = infer_context(max_position_embeddings);
            let params = infer_params_from_name(&name).unwrap_or_else(|| {
                estimate_dense_transformer_params(hidden_size, layers, vocab_size)
            });

            let mut model = Model {
                source: format!("{:?}", ModelType::HF),
                name,
                path: cache_key.clone(),

                quant,
                context,

                layers,
                hidden_size,
                params_billion: round2(params),
                estimates: vec![],
            };

            model.estimates = build_hf_estimates(&model);

            insert_cached_model(&model);
            models.push(model);
        }
    }

    models
}

fn fetch_gguf_models() -> Vec<Model> {
    let mut models = vec![];
    let mut seen = HashSet::new();

    let shard_regex = Regex::new(r"-\d+-of-\d+").unwrap();

    for root in GGUF_ROOTS.iter() {
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            let path_str = path.to_string_lossy().into_owned();

            // reach cache first
            if let Some(model) = get_cached_model(&path_str) {
                models.push(model);
                continue;
            }

            if path.extension().and_then(|s| s.to_str()) != Some("gguf") {
                continue;
            }

            let filename = entry.file_name().to_string_lossy().to_string();
            if shard_regex.is_match(&filename) {
                continue;
            }

            if !seen.insert(filename.clone()) {
                continue;
            }

            let quant = infer_quant_from_name(&filename).unwrap_or(Quant::ALL);
            let params = infer_params_from_name(&filename).unwrap_or(7.0);
            let (layers, hidden_size, context_size) =
                infer_architecture(&path_str).unwrap_or((32, 4096, 32768));
            let context = infer_context(context_size as u64);

            let mut model = Model {
                source: format!("{:?}", ModelType::GGUF),
                name: filename.clone(),
                path: path_str.clone(),

                quant,
                context,

                layers,
                hidden_size,
                params_billion: params,
                estimates: vec![],
            };

            model.estimates = build_gguf_estimates(&model);

            insert_cached_model(&model);
            models.push(model);
        }
    }

    models
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

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn normalize_hf_name(path: &Path) -> String {
    let full = path.to_string_lossy();
    let marker = "models--";

    let idx = match full.find(marker) {
        Some(v) => v,
        None => {
            return path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
        }
    };

    full[idx + marker.len()..].replace("--", "/")
}

fn estimate_dense_transformer_params(hidden: usize, layers: usize, vocab: f64) -> f64 {
    (12.0 * layers as f64 * (hidden as f64).powi(2) + vocab * hidden as f64) / 1e9
}

fn infer_hf_quant(dtype: &str) -> Quant {
    match dtype {
        "bfloat16" => Quant::BF16,
        "float16" => Quant::FP16,
        "float8" => Quant::FP8,
        "int8" => Quant::INT8,
        _ => Quant::ALL,
    }
}

pub fn infer_context(ctx: u64) -> Context {
    match ctx {
        512 => Context::Size512,
        1024 => Context::Size1024,
        2048 => Context::Size2048,
        4096 => Context::Size4096,
        8192 => Context::Size8192,
        16384 => Context::Size16384,
        32768 => Context::Size32768,
        65536 => Context::Size65536,
        131072 => Context::Size131072,
        _ => Context::ALL,
    }
}

pub fn infer_quant_from_name(name: &str) -> Option<Quant> {
    let lower = name.to_lowercase();

    let patterns = [
        (r"(?:^|[-_.])q8_0(?:[-_.]|$)", Quant::Q80),
        (r"(?:^|[-_.])q6_k(?:[-_.]|$)", Quant::Q6K),
        (r"(?:^|[-_.])q5_k_m(?:[-_.]|$)", Quant::Q5KM),
        (r"(?:^|[-_.])q5_0(?:[-_.]|$)", Quant::Q50),
        (r"(?:^|[-_.])q4_k_m(?:[-_.]|$)", Quant::Q4KM),
        (r"(?:^|[-_.])q4_0(?:[-_.]|$)", Quant::Q40),
        (r"(?:^|[-_.])q3_k_m(?:[-_.]|$)", Quant::Q3KM),
        (r"(?:^|[-_.])q2_k(?:[-_.]|$)", Quant::Q2K),
        (r"(?:^|[-_.])fp16(?:[-_.]|$)", Quant::FP16),
        (r"(?:^|[-_.])f16(?:[-_.]|$)", Quant::FP16),
        (r"(?:^|[-_.])bf16(?:[-_.]|$)", Quant::BF16),
        (r"(?:^|[-_.])fp8(?:[-_.]|$)", Quant::FP8),
        (r"(?:^|[-_.])awq(?:[-_.]|$)", Quant::AWQ),
        (r"(?:^|[-_.])gptq(?:[-_.]|$)", Quant::GPTQ),
    ];

    for (pattern, quant) in patterns {
        let re = Regex::new(pattern).ok()?;

        if re.is_match(&lower) {
            return Some(quant);
        }
    }

    None
}

pub fn infer_params_from_name(name: &str) -> Option<f64> {
    Regex::new(r"(?i)(\d+(?:\.\d+)?)b")
        .ok()?
        .captures(name)?
        .get(1)?
        .as_str()
        .parse::<f64>()
        .ok()
}

pub fn infer_architecture(path: &str) -> Option<(usize, usize, usize)> {
    let mut file = File::open(path).ok()?;

    // Read only GGUF header + metadata region.
    let mut bytes = vec![0u8; 16 * 1024 * 1024];

    let read = file.read(&mut bytes).ok()?;
    bytes.truncate(read);

    let gguf = gguf::GGUFFile::read(&bytes).ok()??;

    let metadata = &gguf.header.metadata;

    // ---------------------------------------------------------------------
    // Layers
    // ---------------------------------------------------------------------

    let layers = metadata
        .iter()
        .find(|kv| kv.key.ends_with(".block_count"))
        .and_then(|kv| match &kv.value {
            GGUFMetadataValue::Uint32(v) => Some(*v as usize),
            GGUFMetadataValue::Uint64(v) => Some(*v as usize),
            GGUFMetadataValue::Int32(v) => Some(*v as usize),
            GGUFMetadataValue::Int64(v) => Some(*v as usize),
            _ => None,
        })?;

    // ---------------------------------------------------------------------
    // Hidden size
    // ---------------------------------------------------------------------

    let hidden = metadata
        .iter()
        .find(|kv| kv.key.ends_with(".embedding_length"))
        .and_then(|kv| match &kv.value {
            GGUFMetadataValue::Uint32(v) => Some(*v as usize),
            GGUFMetadataValue::Uint64(v) => Some(*v as usize),
            GGUFMetadataValue::Int32(v) => Some(*v as usize),
            GGUFMetadataValue::Int64(v) => Some(*v as usize),
            _ => None,
        })?;

    // ---------------------------------------------------------------------
    // Hidden size
    // ---------------------------------------------------------------------

    let context = metadata
        .iter()
        .find(|kv| kv.key.ends_with(".context_length"))
        .and_then(|kv| match &kv.value {
            GGUFMetadataValue::Uint32(v) => Some(*v as usize),
            GGUFMetadataValue::Uint64(v) => Some(*v as usize),
            GGUFMetadataValue::Int32(v) => Some(*v as usize),
            GGUFMetadataValue::Int64(v) => Some(*v as usize),
            _ => None,
        })?;

    Some((layers, hidden, context))
}

// ---------------------------------------------------------------------------
// Estimates
// ---------------------------------------------------------------------------

const OVERHEAD_GB: f64 = 3.0;
const FRAGMENTATION_MARGIN: f64 = 1.15;
const KV_BYTES: f64 = 2.0;

fn build_hf_estimates(model: &Model) -> Vec<ModelEstimate> {
    let mut rows = vec![];

    for (quant, _) in Quant::HF_VALUES {
        let w = estimate_weights_gb(model.params_billion, quant.bytes_per_weight());

        for ctx in Context::ALL_VALUES
            .iter()
            .copied()
            .filter(|c| c.as_usize() <= model.context.as_usize())
        {
            let k = estimate_kv_cache_gb(model.layers, model.hidden_size, ctx.as_usize());

            let total = (w + k + OVERHEAD_GB) * FRAGMENTATION_MARGIN;

            rows.push(ModelEstimate {
                quant,
                context: ctx,

                weights_gb: round2(w),
                kv_gb: round2(k),
                total_gb: round2(total),
            });
        }
    }

    rows
}

fn build_gguf_estimates(model: &Model) -> Vec<ModelEstimate> {
    let mut rows = vec![];

    let w = estimate_weights_gb(model.params_billion, model.quant.bytes_per_weight());

    for ctx in Context::ALL_VALUES
        .iter()
        .copied()
        .filter(|c| c.as_usize() <= model.context.as_usize())
    {
        let k = estimate_kv_cache_gb(model.layers, model.hidden_size, ctx.as_usize());

        let total = (w + k + OVERHEAD_GB) * FRAGMENTATION_MARGIN;

        rows.push(ModelEstimate {
            quant: model.quant,
            context: ctx,

            weights_gb: round2(w),
            kv_gb: round2(k),
            total_gb: round2(total),
        });
    }

    rows
}

fn estimate_weights_gb(params_billion: f64, bytes_per_weight: f64) -> f64 {
    (params_billion * 1e9 * bytes_per_weight) / (1024.0f64.powi(3))
}

fn estimate_kv_cache_gb(layers: usize, hidden: usize, context: usize) -> f64 {
    let bytes = 2 * layers * hidden * context * KV_BYTES as usize;
    bytes as f64 / (1024.0f64.powi(3))
}
