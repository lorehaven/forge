use super::store::get_store;
use super::{Context, GGUF_ROOTS, HF_ROOTS, Model, ModelEstimate, ModelType, Quant};
use gguf::GGUFMetadataValue;
use regex::Regex;
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;
use walkdir::WalkDir;

static SHARD_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-\d+-of-\d+").expect("Invalid shard regex"));
static PARAMS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(\d+(?:\.\d+)?)b").expect("Invalid params regex"));

static QUANT_PATTERNS: LazyLock<Vec<(Regex, Quant)>> = LazyLock::new(|| {
    [
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
    ]
    .iter()
    .map(|(p, q)| (Regex::new(p).expect("Invalid quant regex"), *q))
    .collect()
});

#[tracing::instrument(skip(name, quant, context))]
pub async fn fetch_models(
    model_type: ModelType,
    name: &str,
    quant: Quant,
    context: Context,
) -> Vec<Model> {
    let models = match model_type {
        ModelType::HF => fetch_hf_models().await,
        ModelType::GGUF => fetch_gguf_models().await,
    };

    models
        .into_iter()
        .filter(|model| name.is_empty() || model.name.to_lowercase().contains(&name.to_lowercase()))
        .filter(|model| quant == Quant::ALL || model.quant == quant)
        .filter(|model| context == Context::ALL || model.context == context)
        .collect()
}

#[tracing::instrument]
pub async fn fetch_hf_models() -> Vec<Model> {
    let store = get_store();
    let mut models = store.get_all_models().await;
    models.retain(|m| m.source == format!("{:?}", ModelType::HF));

    // Re-verify vLLM support in case architectures list was updated
    for model in models.iter_mut() {
        if let Some(arch) = &model.architecture {
            model.vllm_supported = store.is_vllm_supported(arch);
        }
    }

    let seen: HashSet<String> = models.iter().map(|m| m.path.clone()).collect();

    let mut discovered_models = Vec::new();

    for root in HF_ROOTS.iter() {
        let root = root.to_string();
        if !Path::new(&root).exists() {
            continue;
        }

        let seen_clone = seen.clone();
        let models_batch = tokio::task::spawn_blocking(move || {
            let mut batch = Vec::new();
            for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
                if entry.file_name() != "config.json" {
                    continue;
                }

                let config_path = entry.path();
                let model_dir = match config_path.ancestors().nth(3) {
                    Some(p) => p,
                    None => continue,
                };

                let path_str = model_dir.to_string_lossy().to_string();
                if seen_clone.contains(&path_str) {
                    continue;
                }

                let name = normalize_hf_name(model_dir);

                let content = match std::fs::read_to_string(config_path) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("Failed to read config.json at {:?}: {}", config_path, e);
                        continue;
                    }
                };

                let json: serde_json::Value = match serde_json::from_str(&content) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("Failed to parse config.json at {:?}: {}", config_path, e);
                        continue;
                    }
                };

                let hidden_size = json["hidden_size"].as_u64().unwrap_or(4096) as usize;
                let layers = json["num_hidden_layers"].as_u64().unwrap_or(32) as usize;
                let vocab_size = json["vocab_size"].as_u64().unwrap_or(32000) as f64;
                let max_position_embeddings =
                    json["max_position_embeddings"].as_u64().unwrap_or(4096);
                let torch_dtype = json["torch_dtype"].as_str().unwrap_or("float16");
                let architecture = json["architectures"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let quant = infer_hf_quant(torch_dtype);
                let context = infer_context(max_position_embeddings);
                let params = infer_params_from_name(&name).unwrap_or_else(|| {
                    estimate_dense_transformer_params(hidden_size, layers, vocab_size)
                });

                batch.push((
                    name,
                    path_str,
                    architecture,
                    quant,
                    context,
                    layers,
                    hidden_size,
                    params,
                ));
            }
            batch
        })
        .await
        .unwrap_or_default();

        for (name, path, architecture, quant, context, layers, hidden_size, params) in models_batch
        {
            let vllm_supported = architecture
                .as_ref()
                .map(|arch| store.is_vllm_supported(arch))
                .unwrap_or(false);

            let mut model = Model {
                source: format!("{:?}", ModelType::HF),
                name,
                path,
                architecture,
                vllm_supported,
                quant,
                context,
                layers,
                hidden_size,
                params_billion: round2(params),
                estimates: vec![],
            };

            model.estimates = build_hf_estimates(&model);
            store.insert_model(&model).await;
            discovered_models.push(model);
        }
    }

    if !discovered_models.is_empty() {
        tracing::info!("Discovered {} new HF models.", discovered_models.len());
    }

    models.extend(discovered_models);
    models
}

#[tracing::instrument]
pub async fn fetch_gguf_models() -> Vec<Model> {
    let store = get_store();
    let mut models = store.get_all_models().await;
    models.retain(|m| m.source == format!("{:?}", ModelType::GGUF));

    let seen: HashSet<String> = models.iter().map(|m| m.path.clone()).collect();
    let mut discovered_models = Vec::new();

    for root in GGUF_ROOTS.iter() {
        let root = root.to_string();
        if !Path::new(&root).exists() {
            continue;
        }

        let seen_clone = seen.clone();
        let models_batch = tokio::task::spawn_blocking(move || {
            let mut batch = Vec::new();
            for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
                let path = entry.path();
                let path_str = path.to_string_lossy().into_owned();

                if seen_clone.contains(&path_str) {
                    continue;
                }

                if path.extension().and_then(|s| s.to_str()) != Some("gguf") {
                    continue;
                }

                let filename = entry.file_name().to_string_lossy().to_string();
                if SHARD_REGEX.is_match(&filename) {
                    continue;
                }

                let quant = infer_quant_from_name(&filename).unwrap_or(Quant::ALL);
                let params = infer_params_from_name(&filename).unwrap_or(7.0);

                let arch_info = infer_architecture(&path_str);

                batch.push((filename, path_str, quant, params, arch_info));
            }
            batch
        })
        .await
        .unwrap_or_default();

        for (filename, path_str, quant, params, arch_info) in models_batch {
            let (layers, hidden_size, context_size) = arch_info.unwrap_or((32, 4096, 32768));
            let context = infer_context(context_size as u64);

            let mut model = Model {
                source: format!("{:?}", ModelType::GGUF),
                name: filename,
                path: path_str,
                architecture: None,
                vllm_supported: false,
                quant,
                context,
                layers,
                hidden_size,
                params_billion: params,
                estimates: vec![],
            };

            model.estimates = build_gguf_estimates(&model);
            store.insert_model(&model).await;
            discovered_models.push(model);
        }
    }

    if !discovered_models.is_empty() {
        tracing::info!("Discovered {} new GGUF models.", discovered_models.len());
    }

    models.extend(discovered_models);
    models
}

pub fn get_on_disk_model_paths() -> HashSet<String> {
    let mut on_disk_paths = HashSet::new();

    // HF models
    for root in HF_ROOTS.iter() {
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if entry.file_name() == "config.json"
                && let Some(model_dir) = entry.path().ancestors().nth(3)
            {
                on_disk_paths.insert(model_dir.to_string_lossy().to_string());
            }
        }
    }

    // GGUF models
    for root in GGUF_ROOTS.iter() {
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("gguf") {
                let filename = entry.file_name().to_string_lossy().to_string();
                if !SHARD_REGEX.is_match(&filename) {
                    on_disk_paths.insert(path.to_string_lossy().to_string());
                }
            }
        }
    }

    on_disk_paths
}

pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

pub fn normalize_hf_name(path: &Path) -> String {
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

pub fn estimate_dense_transformer_params(hidden: usize, layers: usize, vocab: f64) -> f64 {
    (12.0 * layers as f64 * (hidden as f64).powi(2) + vocab * hidden as f64) / 1e9
}

pub fn infer_hf_quant(dtype: &str) -> Quant {
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

    for (re, quant) in QUANT_PATTERNS.iter() {
        if re.is_match(&lower) {
            return Some(*quant);
        }
    }

    None
}

pub fn infer_params_from_name(name: &str) -> Option<f64> {
    PARAMS_REGEX
        .captures(name)?
        .get(1)?
        .as_str()
        .parse::<f64>()
        .ok()
}

pub fn infer_architecture(path: &str) -> Option<(usize, usize, usize)> {
    let mut file = File::open(path).ok()?;

    // Read only GGUF header + metadata region.
    // Reduced from 16MB to 1MB as suggested in the plan.
    let mut bytes = vec![0u8; 1024 * 1024];

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
    // Context length
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

const OVERHEAD_GB: f64 = 3.0;
const FRAGMENTATION_MARGIN: f64 = 1.15;
const KV_BYTES: f64 = 2.0;

pub fn build_hf_estimates(model: &Model) -> Vec<ModelEstimate> {
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

pub fn build_gguf_estimates(model: &Model) -> Vec<ModelEstimate> {
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

pub fn estimate_weights_gb(params_billion: f64, bytes_per_weight: f64) -> f64 {
    (params_billion * 1e9 * bytes_per_weight) / (1024.0f64.powi(3))
}

pub fn estimate_kv_cache_gb(layers: usize, hidden: usize, context: usize) -> f64 {
    let bytes = 2 * layers * hidden * context * KV_BYTES as usize;
    bytes as f64 / (1024.0f64.powi(3))
}
