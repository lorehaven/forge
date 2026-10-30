//! Direct coverage of `routers/models/list.rs`'s `apply_filters` (pure, and already `pub`).

use switchboard_service::routers::gpu::monitor::GpuInfo;
use switchboard_service::routers::models::list::apply_filters;
use switchboard_service::routers::models::types::{
    Context, Model, ModelEstimate, ModelFilters, Quant,
};

fn gpu() -> GpuInfo {
    GpuInfo {
        name: "test-gpu".to_string(),
        total_gb: 24.0,
        used_gb: 4.0,
        free_gb: 20.0,
    }
}

fn model(name: &str, source: &str, quant: Quant, context: Context, params: f64) -> Model {
    Model {
        source: source.to_string(),
        name: name.to_string(),
        path: format!("/tmp/list-test/{name}"),
        architecture: None,
        vllm_supported: false,
        quant,
        context,
        layers: 32,
        hidden_size: 4096,
        params_billion: params,
        estimates: vec![],
    }
}

const fn filters() -> ModelFilters {
    ModelFilters {
        source: None,
        search: None,
        sort: None,
        quant: None,
        context: None,
        vllm_only: None,
    }
}

// -----------------------------------------------------------------
// apply_filters
// -----------------------------------------------------------------

#[quench_test::test]
fn apply_filters_defaults_the_source_to_hf() {
    let mut models = vec![
        model("a", "HF", Quant::FP16, Context::Size4096, 7.0),
        model("b", "GGUF", Quant::FP16, Context::Size4096, 7.0),
    ];
    apply_filters(&mut models, &filters(), &gpu());
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "a");
}

#[quench_test::test]
fn apply_filters_source_all_keeps_every_source() {
    let mut models = vec![
        model("a", "HF", Quant::FP16, Context::Size4096, 7.0),
        model("b", "GGUF", Quant::FP16, Context::Size4096, 7.0),
    ];
    let mut f = filters();
    f.source = Some("all".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 2);
}

#[quench_test::test]
fn apply_filters_matches_source_case_insensitively() {
    let mut models = vec![model("a", "GGUF", Quant::FP16, Context::Size4096, 7.0)];
    let mut f = filters();
    f.source = Some("gguf".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);
}

#[quench_test::test]
fn apply_filters_by_search_term_is_case_insensitive_and_substring() {
    let mut models = vec![
        model("Llama Three", "HF", Quant::FP16, Context::Size4096, 7.0),
        model("Mistral", "HF", Quant::FP16, Context::Size4096, 7.0),
    ];
    let mut f = filters();
    f.search = Some("llama".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "Llama Three");
}

#[quench_test::test]
fn apply_filters_by_search_ignores_an_empty_string() {
    let mut models = vec![
        model("a", "HF", Quant::FP16, Context::Size4096, 7.0),
        model("b", "HF", Quant::FP16, Context::Size4096, 7.0),
    ];
    let mut f = filters();
    f.search = Some(String::new());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 2);
}

#[quench_test::test]
fn apply_filters_by_quant_matches_debug_format_or_known_alias() {
    let mut models = vec![
        model("a", "HF", Quant::Q80, Context::Size4096, 7.0),
        model("b", "HF", Quant::FP16, Context::Size4096, 7.0),
    ];
    let mut f = filters();
    f.quant = Some("Q8_0".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].quant, Quant::Q80);
}

#[quench_test::test]
fn apply_filters_by_quant_all_or_empty_keeps_everything() {
    let mut models = vec![
        model("a", "HF", Quant::Q80, Context::Size4096, 7.0),
        model("b", "HF", Quant::FP16, Context::Size4096, 7.0),
    ];
    let mut f = filters();
    f.quant = Some("ALL".to_string());
    apply_filters(&mut models.clone(), &f, &gpu());
    f.quant = Some(String::new());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 2);
}

#[quench_test::test]
fn apply_filters_by_context_as_number_and_string() {
    let mut models = vec![
        model("small", "HF", Quant::FP16, Context::Size4096, 7.0),
        model("big", "HF", Quant::FP16, Context::Size32768, 7.0),
    ];
    let mut f = filters();
    f.context = Some(quench_serde::json!(8192));
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "big");

    let mut models = vec![
        model("small", "HF", Quant::FP16, Context::Size4096, 7.0),
        model("big", "HF", Quant::FP16, Context::Size32768, 7.0),
    ];
    let mut f = filters();
    f.context = Some(quench_serde::json!("8192"));
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "big");
}

#[quench_test::test]
fn apply_filters_by_context_zero_or_unparseable_keeps_everything() {
    let mut models = vec![model("a", "HF", Quant::FP16, Context::Size4096, 7.0)];
    let mut f = filters();
    f.context = Some(quench_serde::json!(0));
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);

    let mut models = vec![model("a", "HF", Quant::FP16, Context::Size4096, 7.0)];
    let mut f = filters();
    f.context = Some(quench_serde::json!(true));
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);
}

#[quench_test::test]
fn apply_filters_by_vllm_only() {
    let mut supported = model("a", "HF", Quant::FP16, Context::Size4096, 7.0);
    supported.vllm_supported = true;
    let mut models = vec![
        supported,
        model("b", "HF", Quant::FP16, Context::Size4096, 7.0),
    ];
    let mut f = filters();
    f.vllm_only = Some(true);
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "a");
}

#[quench_test::test]
fn apply_filters_sorts_by_name_asc_and_desc() {
    let mut models = vec![
        model("b", "HF", Quant::FP16, Context::Size4096, 7.0),
        model("a", "HF", Quant::FP16, Context::Size4096, 7.0),
    ];
    let mut f = filters();
    f.sort = Some("name_asc".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models[0].name, "a");

    f.sort = Some("name_desc".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models[0].name, "b");
}

#[quench_test::test]
fn apply_filters_sorts_by_params_asc_and_desc() {
    let mut models = vec![
        model("big", "HF", Quant::FP16, Context::Size4096, 70.0),
        model("small", "HF", Quant::FP16, Context::Size4096, 7.0),
    ];
    let mut f = filters();
    f.sort = Some("params_asc".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models[0].name, "small");

    f.sort = Some("params_desc".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models[0].name, "big");
}

fn model_with_estimate(name: &str, total_gb: f64) -> Model {
    let mut m = model(name, "HF", Quant::FP16, Context::Size4096, 7.0);
    m.estimates = vec![ModelEstimate {
        quant: Quant::FP16,
        context: Context::Size4096,
        weights_gb: total_gb - 1.0,
        kv_gb: 1.0,
        total_gb,
    }];
    m
}

#[quench_test::test]
fn apply_filters_sorts_by_vram_asc_and_desc_using_the_best_fitting_estimate() {
    let mut models = vec![
        model_with_estimate("high", 18.0),
        model_with_estimate("low", 4.0),
    ];
    let mut f = filters();
    f.sort = Some("vram_asc".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models[0].name, "low");

    f.sort = Some("vram_desc".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models[0].name, "high");
}

#[quench_test::test]
fn apply_filters_unknown_sort_key_leaves_order_unchanged() {
    let mut models = vec![
        model("b", "HF", Quant::FP16, Context::Size4096, 7.0),
        model("a", "HF", Quant::FP16, Context::Size4096, 7.0),
    ];
    let mut f = filters();
    f.sort = Some("not-a-real-sort".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models[0].name, "b");
}

// -----------------------------------------------------------------
// handle_grid / estimates_modal - exercise the private render helpers
// (find_best_estimate/find_minimum_estimate/render_model_grid/
// render_estimates_modal) transitively, the way handlers_tests already does
// for the simpler no-estimates case.
// -----------------------------------------------------------------
