use super::store::get_store;
use super::{Context, GGUF_ROOTS, HF_ROOTS, Model, ModelEstimate, ModelType, Quant};
use quench_core::fs::Walk;
use quench_regex::Regex;
use std::collections::HashSet;
use std::fs::File;
use std::path::Path;
use std::sync::LazyLock;

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

#[quench_log::instrument(skip(name, quant, context))]
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

/// What one `config.json` says about the model beside it.
struct ScannedModel {
    name: String,
    path: String,
    architecture: Option<String>,
    quant: Quant,
    context: Context,
    layers: usize,
    hidden_size: usize,
    params: f64,
}

/// Reads one model's `config.json`; `None` (with a warning) when it cannot be read or parsed.
fn read_hf_config(config_path: &Path) -> Option<ScannedModel> {
    let model_dir = config_path.ancestors().nth(3)?;
    let path = model_dir.to_string_lossy().to_string();
    let name = normalize_hf_name(model_dir);

    let config_json_text = std::fs::read_to_string(config_path)
        .inspect_err(|e| quench_log::warn!("Failed to read config.json at {config_path:?}: {e}"))
        .ok()?;
    let json: quench_serde::Value = quench_serde::json::from_str(&config_json_text)
        .inspect_err(|e| quench_log::warn!("Failed to parse config.json at {config_path:?}: {e}"))
        .ok()?;

    let hidden_size = usize::try_from(json["hidden_size"].as_u64().unwrap_or(4096)).unwrap_or(4096);
    let layers = usize::try_from(json["num_hidden_layers"].as_u64().unwrap_or(32)).unwrap_or(32);
    let vocab_size =
        f64::from(u32::try_from(json["vocab_size"].as_u64().unwrap_or(32000)).unwrap_or(32000));
    let max_position_embeddings = json["max_position_embeddings"].as_u64().unwrap_or(4096);
    let torch_dtype = json["torch_dtype"].as_str().unwrap_or("float16");
    let architecture = json["architectures"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);

    let quant = infer_hf_quant_config(&json).unwrap_or_else(|| infer_hf_quant(torch_dtype));
    let context = infer_context(max_position_embeddings);
    let params = infer_params_from_name(&name)
        .unwrap_or_else(|| estimate_dense_transformer_params(hidden_size, layers, vocab_size));

    Some(ScannedModel {
        name,
        path,
        architecture,
        quant,
        context,
        layers,
        hidden_size,
        params,
    })
}

/// Every model under `root`, found by its `config.json`.
fn scan_hf_root(root: &str) -> Vec<ScannedModel> {
    Walk::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() == "config.json")
        .filter_map(|entry| read_hf_config(entry.path()))
        .collect()
}

#[quench_log::instrument]
pub async fn fetch_hf_models() -> Vec<Model> {
    let store = get_store();
    let mut models = store.get_all_models().await;
    models.retain(|m| m.source == format!("{:?}", ModelType::HF));

    // Re-verify vLLM support in case architectures list was updated
    for model in &mut models {
        if let Some(arch) = &model.architecture {
            model.vllm_supported = store.is_vllm_supported(arch);
        }
    }

    let mut discovered_models = Vec::new();

    for root in HF_ROOTS.iter() {
        let root = root.clone();
        if !Path::new(&root).exists() {
            continue;
        }

        let models_batch = tokio::task::spawn_blocking(move || scan_hf_root(&root))
            .await
            .unwrap_or_default();

        for ScannedModel {
            name,
            path,
            architecture,
            quant,
            context,
            layers,
            hidden_size,
            params,
        } in models_batch
        {
            let vllm_supported = architecture
                .as_ref()
                .is_some_and(|arch| store.is_vllm_supported(arch));

            // Known models are re-read too, so improvements to inference (e.g. picking up
            // `quantization_config`) reach entries cached by an older build. Unchanged
            // ones are left alone to avoid a DB write per model every sync.
            if let Some(existing) = models.iter().find(|m| m.path == path)
                && existing.quant == quant
                && existing.context == context
                && existing.layers == layers
                && existing.hidden_size == hidden_size
                && existing.params_billion.total_cmp(&round2(params)).is_eq()
            {
                continue;
            }

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
        quench_log::info!(
            "Discovered or refreshed {} HF models.",
            discovered_models.len()
        );
        models.retain(|m| !discovered_models.iter().any(|d| d.path == m.path));
    }

    models.extend(discovered_models);
    models
}

#[quench_log::instrument]
pub async fn fetch_gguf_models() -> Vec<Model> {
    let store = get_store();
    let mut models = store.get_all_models().await;
    models.retain(|m| m.source == format!("{:?}", ModelType::GGUF));

    let seen: HashSet<String> = models.iter().map(|m| m.path.clone()).collect();
    let mut discovered_models = Vec::new();

    for root in GGUF_ROOTS.iter() {
        let root = root.clone();
        if !Path::new(&root).exists() {
            continue;
        }

        let seen_clone = seen.clone();
        let models_batch = tokio::task::spawn_blocking(move || {
            let mut batch = Vec::new();
            for entry in Walk::new(&root).into_iter().filter_map(Result::ok) {
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
        quench_log::info!("Discovered {} new GGUF models.", discovered_models.len());
    }

    models.extend(discovered_models);
    models
}

pub fn get_on_disk_model_paths() -> HashSet<String> {
    let mut on_disk_paths = HashSet::new();

    for root in HF_ROOTS.iter() {
        for entry in Walk::new(root).into_iter().filter_map(Result::ok) {
            if entry.file_name() == "config.json"
                && let Some(model_dir) = entry.path().ancestors().nth(3)
            {
                on_disk_paths.insert(model_dir.to_string_lossy().to_string());
            }
        }
    }

    for root in GGUF_ROOTS.iter() {
        for entry in Walk::new(root).into_iter().filter_map(Result::ok) {
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

#[must_use]
pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[must_use]
pub fn normalize_hf_name(path: &Path) -> String {
    let full = path.to_string_lossy();
    let marker = "models--";

    let Some(idx) = full.find(marker) else {
        return path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
    };

    full[idx + marker.len()..].replace("--", "/")
}

/// `n` as an `f64`, exact up to 2^53 (far past any layer, width or context size).
fn usize_to_f64(n: usize) -> f64 {
    let n = u64::try_from(n).unwrap_or(u64::MAX);
    let high = u32::try_from(n >> 32).unwrap_or(u32::MAX);
    let low = u32::try_from(n & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    f64::from(high) * 4_294_967_296.0 + f64::from(low)
}

#[must_use]
pub fn estimate_dense_transformer_params(hidden: usize, layers: usize, vocab: f64) -> f64 {
    let hidden = usize_to_f64(hidden);
    vocab.mul_add(hidden, 12.0 * usize_to_f64(layers) * hidden.powi(2)) / 1e9
}

#[must_use]
pub fn infer_hf_quant(dtype: &str) -> Quant {
    match dtype {
        "bfloat16" => Quant::BF16,
        "float16" => Quant::FP16,
        "float8" => Quant::FP8,
        "int8" => Quant::INT8,
        _ => Quant::ALL,
    }
}

/// Infers a model's quantization from its checkpoint metadata.
///
/// Quant declared by a pre-quantized checkpoint's `quantization_config`, which takes
/// precedence over `torch_dtype` (that only describes the activation dtype, so a 4-bit
/// build would otherwise be sized as a 2-byte-per-weight model).
#[must_use]
pub fn infer_hf_quant_config(config: &quench_serde::Value) -> Option<Quant> {
    let qc = config.get("quantization_config")?;
    match qc["quant_method"].as_str()? {
        "awq" => Some(Quant::AWQ),
        "gptq" => Some(Quant::GPTQ),
        "fp8" => Some(Quant::FP8),
        "bitsandbytes" => Some(Quant::INT8),
        "compressed-tensors" => {
            // llm-compressor output: weight precision lives in the first config group.
            let weights = qc["config_groups"]
                .as_object()?
                .values()
                .find_map(|group| group.get("weights"))?;
            match (weights["num_bits"].as_u64()?, weights["type"].as_str()) {
                (4, _) => Some(Quant::AWQ),
                (8, Some("float")) => Some(Quant::FP8),
                (8, _) => Some(Quant::INT8),
                _ => None,
            }
        }
        _ => None,
    }
}

#[must_use]
pub const fn infer_context(ctx: u64) -> Context {
    match ctx {
        512 => Context::Size512,
        1024 => Context::Size1024,
        2048 => Context::Size2048,
        4096 => Context::Size4096,
        8192 => Context::Size8192,
        16384 => Context::Size16384,
        32768 => Context::Size32768,
        65536 => Context::Size65536,
        131_072 => Context::Size131072,
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

#[must_use]
pub fn infer_architecture(path: &str) -> Option<(usize, usize, usize)> {
    let file = File::open(path).ok()?;
    // Only the header and metadata are read; the weights that follow are never touched.
    let header =
        quench_convert::gguf::read(file, &quench_convert::gguf::Options::default()).ok()?;
    let metadata = header.get("metadata")?.as_object()?;

    let find = |suffix: &str| {
        metadata
            .iter()
            .find(|(key, _)| key.ends_with(suffix))
            .and_then(|(_, value)| match value {
                quench_serde::Value::UInt(v) => usize::try_from(*v).ok(),
                quench_serde::Value::Int(v) => usize::try_from(*v).ok(),
                _ => None,
            })
    };

    Some((
        find(".block_count")?,
        find(".embedding_length")?,
        find(".context_length")?,
    ))
}

const OVERHEAD_GB: f64 = 3.0;
const FRAGMENTATION_MARGIN: f64 = 1.15;
const KV_BYTES: f64 = 2.0;

#[must_use]
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

#[must_use]
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

#[must_use]
pub fn estimate_weights_gb(params_billion: f64, bytes_per_weight: f64) -> f64 {
    (params_billion * 1e9 * bytes_per_weight) / (1024.0f64.powi(3))
}

#[must_use]
pub fn estimate_kv_cache_gb(layers: usize, hidden: usize, context: usize) -> f64 {
    let bytes =
        2.0 * KV_BYTES * usize_to_f64(layers) * usize_to_f64(hidden) * usize_to_f64(context);
    bytes / (1024.0f64.powi(3))
}
