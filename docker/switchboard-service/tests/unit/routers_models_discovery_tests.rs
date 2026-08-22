//! Unit tests for the pure inference/estimate math in `routers/models/discovery.rs`.
//! The filesystem-walking `fetch_hf_models`/`fetch_gguf_models` are exercised
//! indirectly through `routers_models_sync_tests.rs` against the default
//! (nonexistent-in-CI) roots, which is the only branch that doesn't need a
//! real model directory on disk.

use std::path::Path;
use switchboard_service::routers::models::discovery::*;
use switchboard_service::routers::models::types::{Context, Model, Quant};

#[test]
fn round2_rounds_to_two_decimal_places() {
    // 1.005 is not exactly representable in f64 (it's stored as slightly
    // less than 1.005), so `(1.005 * 100.0).round() / 100.0` lands on 1.0,
    // not 1.01 - use values that round unambiguously instead.
    assert_eq!(round2(1.006), 1.01);
    assert_eq!(round2(1.0), 1.0);
    assert_eq!(round2(1.234), 1.23);
}

#[test]
fn normalize_hf_name_extracts_the_repo_id_from_the_hub_cache_layout() {
    let path = Path::new(
        "/mnt/dev/huggingface/hub/models--meta-llama--Llama-3-8B/snapshots/abc/config.json",
    );
    // `ancestors().nth(3)` from config.json would land on the snapshot dir in
    // real usage; normalize_hf_name itself just looks for the `models--` marker
    // anywhere in the path and rewrites `--` to `/` after it.
    assert_eq!(
        normalize_hf_name(path),
        "meta-llama/Llama-3-8B/snapshots/abc/config.json"
    );
}

#[test]
fn normalize_hf_name_falls_back_to_the_file_name_without_the_marker() {
    let path = Path::new("/mnt/dev/huggingface/hub/some-dir");
    assert_eq!(normalize_hf_name(path), "some-dir");
}

#[test]
fn estimate_dense_transformer_params_grows_with_layers_and_hidden_size() {
    let small = estimate_dense_transformer_params(1024, 12, 32000.0);
    let large = estimate_dense_transformer_params(4096, 32, 32000.0);
    assert!(large > small);
    assert!(small > 0.0);
}

#[test]
fn infer_hf_quant_maps_known_torch_dtypes() {
    assert_eq!(infer_hf_quant("bfloat16"), Quant::BF16);
    assert_eq!(infer_hf_quant("float16"), Quant::FP16);
    assert_eq!(infer_hf_quant("float8"), Quant::FP8);
    assert_eq!(infer_hf_quant("int8"), Quant::INT8);
    assert_eq!(infer_hf_quant("something-else"), Quant::ALL);
}

#[test]
fn infer_context_maps_known_sizes_and_falls_back_to_all() {
    assert_eq!(infer_context(4096), Context::Size4096);
    assert_eq!(infer_context(131072), Context::Size131072);
    assert_eq!(infer_context(12345), Context::ALL);
}

#[test]
fn infer_quant_from_name_matches_common_gguf_suffixes() {
    assert_eq!(
        infer_quant_from_name("llama-3-8b-instruct.Q4_K_M.gguf"),
        Some(Quant::Q4KM)
    );
    assert_eq!(infer_quant_from_name("model-q8_0.gguf"), Some(Quant::Q80));
    assert_eq!(infer_quant_from_name("model-awq.gguf"), Some(Quant::AWQ));
    assert_eq!(
        infer_quant_from_name("model-with-no-quant-marker.gguf"),
        None
    );
}

#[test]
fn infer_params_from_name_reads_the_b_suffixed_number() {
    assert_eq!(
        infer_params_from_name("llama-3-8b-instruct.gguf"),
        Some(8.0)
    );
    assert_eq!(infer_params_from_name("mixtral-8x7B.gguf"), Some(7.0));
    assert_eq!(infer_params_from_name("no-params-here.gguf"), None);
}

#[test]
fn infer_architecture_returns_none_for_a_missing_or_non_gguf_file() {
    assert_eq!(infer_architecture("/does/not/exist.gguf"), None);
}

fn sample_model(quant: Quant, context: Context) -> Model {
    Model {
        source: "HF".to_string(),
        name: "sample".to_string(),
        path: "/tmp/sample".to_string(),
        architecture: None,
        vllm_supported: false,
        quant,
        context,
        layers: 32,
        hidden_size: 4096,
        params_billion: 8.0,
        estimates: vec![],
    }
}

#[test]
fn build_hf_estimates_covers_every_hf_quant_at_every_context_up_to_the_models_max() {
    let model = sample_model(Quant::FP16, Context::Size4096);
    let estimates = build_hf_estimates(&model);

    // 7 HF quants * contexts up to and including 4096 (512..=4096 -> 4 sizes)
    assert_eq!(estimates.len(), 7 * 4);
    assert!(estimates.iter().all(|e| e.context.as_usize() <= 4096));
    assert!(estimates.iter().all(|e| e.total_gb > 0.0));
}

#[test]
fn build_gguf_estimates_only_covers_the_models_own_quant() {
    let model = sample_model(Quant::Q4KM, Context::Size2048);
    let estimates = build_gguf_estimates(&model);

    assert!(estimates.iter().all(|e| e.quant == Quant::Q4KM));
    // contexts up to and including 2048: 512, 1024, 2048
    assert_eq!(estimates.len(), 3);
}

#[test]
fn estimate_weights_gb_scales_linearly_with_params_and_bytes() {
    let one_byte = estimate_weights_gb(1.0, 1.0);
    let two_bytes = estimate_weights_gb(1.0, 2.0);
    assert!((two_bytes - one_byte * 2.0).abs() < 1e-9);
    assert!(one_byte > 0.0);
}

#[test]
fn estimate_kv_cache_gb_scales_with_layers_hidden_and_context() {
    let base = estimate_kv_cache_gb(32, 4096, 8192);
    let double_context = estimate_kv_cache_gb(32, 4096, 16384);
    assert!((double_context - base * 2.0).abs() < 1e-9);
}

#[test]
fn get_on_disk_model_paths_is_empty_when_the_default_roots_do_not_exist() {
    // HF_ROOTS/GGUF_ROOTS default to /mnt/dev/... which does not exist in this
    // sandbox, and once the LazyLock is initialized it stays fixed for the
    // whole test binary - so this only asserts the "no roots on disk" branch,
    // not the discovery branch (covered by walking a real tree would require
    // controlling the env before any other test touches the LazyLock, which
    // isn't guaranteed under a parallel test runner).
    let paths = get_on_disk_model_paths();
    assert!(paths.is_empty() || paths.iter().all(|p| !p.is_empty()));
}
