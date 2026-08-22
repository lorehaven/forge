//! The launch-modal fit/GPU-utilization math in `routers/vllm/modals.rs`,
//! plus light coverage of the handlers and render functions.

use actix_web::App;
use actix_web::test as actix_test;
use switchboard_service::routers::gpu::monitor::GpuInfo;
use switchboard_service::routers::models::store::{get_store, init_model_store};
use switchboard_service::routers::models::types::{Context, Model, ModelEstimate, Quant};
use switchboard_service::routers::vllm::modals::{
    LaunchModalQuery, calculate_minimum_gpu_util, empty_launch_modal, empty_stop_modal,
    find_launch_estimate, fit_note_icon_class, get_vllm_namespace, handle_launch_modal,
    handle_stop_modal, launch_fit_note, launch_gpu_util, parse_optional_f32, parse_optional_u32,
    render_launch_modal, render_stop_modal, round_gpu_util_up,
};

fn gpu(total_gb: f64, free_gb: f64) -> GpuInfo {
    GpuInfo {
        name: "test-gpu".to_string(),
        total_gb,
        used_gb: total_gb - free_gb,
        free_gb,
    }
}

fn estimate(quant: Quant, context: Context, weights_gb: f64, kv_gb: f64) -> ModelEstimate {
    ModelEstimate {
        quant,
        context,
        weights_gb,
        kv_gb,
        total_gb: weights_gb + kv_gb,
    }
}

fn model_with_estimates(estimates: Vec<ModelEstimate>) -> Model {
    Model {
        source: "HF".to_string(),
        name: "test-model".to_string(),
        path: "/tmp/test-model".to_string(),
        architecture: None,
        vllm_supported: true,
        quant: Quant::FP16,
        context: Context::Size4096,
        layers: 32,
        hidden_size: 4096,
        params_billion: 7.0,
        estimates,
    }
}

// ---------------------------------------------------------------------------
// parse_optional_{u32,f32}
// ---------------------------------------------------------------------------

#[test]
fn parse_optional_u32_and_f32_trim_and_reject_blank_or_invalid() {
    assert_eq!(parse_optional_u32(Some(" 8192 ")), Some(8192));
    assert_eq!(parse_optional_u32(Some("")), None);
    assert_eq!(parse_optional_u32(Some("nope")), None);
    assert_eq!(parse_optional_u32(None), None);

    assert_eq!(parse_optional_f32(Some(" 0.85 ")), Some(0.85));
    assert_eq!(parse_optional_f32(Some("")), None);
    assert_eq!(parse_optional_f32(None), None);
}

// ---------------------------------------------------------------------------
// round_gpu_util_up / calculate_minimum_gpu_util
// ---------------------------------------------------------------------------

#[test]
fn round_gpu_util_up_rounds_to_the_next_twentieth() {
    assert_eq!(round_gpu_util_up(0.81), 0.85);
    assert_eq!(round_gpu_util_up(0.80), 0.80); // already exact
    assert_eq!(round_gpu_util_up(0.001), 0.05);
}

#[test]
fn calculate_minimum_gpu_util_covers_the_estimate_against_the_gpu_with_a_safety_margin() {
    let est = estimate(Quant::FP16, Context::Size4096, 14.0, 2.0);
    // (14 + 1.5) / (24 - 2) = 0.704545... -> rounds up to 0.75
    let result = calculate_minimum_gpu_util(&est, 24.0).unwrap();
    assert_eq!(result, 0.75);
}

#[test]
fn calculate_minimum_gpu_util_is_none_for_a_non_positive_or_non_finite_gpu_total() {
    let est = estimate(Quant::FP16, Context::Size4096, 14.0, 2.0);
    assert_eq!(calculate_minimum_gpu_util(&est, 0.0), None);
    assert_eq!(calculate_minimum_gpu_util(&est, -1.0), None);
    assert_eq!(calculate_minimum_gpu_util(&est, f64::NAN), None);
    assert_eq!(calculate_minimum_gpu_util(&est, f64::INFINITY), None);
}

#[test]
fn calculate_minimum_gpu_util_is_none_when_kv_cache_alone_exceeds_the_gpu() {
    let est = estimate(Quant::FP16, Context::Size4096, 14.0, 30.0);
    assert_eq!(calculate_minimum_gpu_util(&est, 24.0), None);
}

#[test]
fn calculate_minimum_gpu_util_clamps_to_the_20_to_100_percent_range() {
    // Tiny model on a huge GPU: raw ratio would round to far less than 20%.
    let tiny = estimate(Quant::FP16, Context::Size4096, 0.01, 0.01);
    assert_eq!(calculate_minimum_gpu_util(&tiny, 200.0), Some(0.20));

    // Model barely fits at all: raw ratio would exceed 100%.
    let huge = estimate(Quant::FP16, Context::Size4096, 22.0, 1.0);
    assert_eq!(calculate_minimum_gpu_util(&huge, 24.0), Some(1.0));
}

// ---------------------------------------------------------------------------
// find_launch_estimate
// ---------------------------------------------------------------------------

#[test]
fn find_launch_estimate_filters_by_mapped_quantization_name() {
    let model = model_with_estimates(vec![
        estimate(Quant::AWQ, Context::Size4096, 5.0, 1.0),
        estimate(Quant::FP16, Context::Size4096, 14.0, 2.0),
    ]);
    let found = find_launch_estimate(&model, "awq", None).unwrap();
    assert_eq!(found.quant, Quant::AWQ);
}

#[test]
fn find_launch_estimate_maps_marlin_aliases_to_the_same_quant() {
    let model = model_with_estimates(vec![estimate(Quant::AWQ, Context::Size4096, 5.0, 1.0)]);
    assert!(find_launch_estimate(&model, "awq_marlin", None).is_some());

    let model = model_with_estimates(vec![estimate(Quant::GPTQ, Context::Size4096, 5.0, 1.0)]);
    assert!(find_launch_estimate(&model, "gptq_marlin", None).is_some());

    let model = model_with_estimates(vec![estimate(Quant::FP8, Context::Size4096, 5.0, 1.0)]);
    assert!(find_launch_estimate(&model, "fp8", None).is_some());

    let model = model_with_estimates(vec![estimate(Quant::INT8, Context::Size4096, 5.0, 1.0)]);
    assert!(find_launch_estimate(&model, "bitsandbytes", None).is_some());
}

#[test]
fn find_launch_estimate_ignores_unknown_quantization_strings() {
    let model = model_with_estimates(vec![estimate(Quant::FP16, Context::Size4096, 14.0, 2.0)]);
    // "unknown-quant" maps to no known Quant, so the filter is skipped
    // entirely rather than matching nothing.
    assert!(find_launch_estimate(&model, "unknown-quant", None).is_some());
}

#[test]
fn find_launch_estimate_picks_the_smallest_context_at_or_above_the_requested_one() {
    let model = model_with_estimates(vec![
        estimate(Quant::FP16, Context::Size2048, 14.0, 1.0),
        estimate(Quant::FP16, Context::Size4096, 14.0, 2.0),
        estimate(Quant::FP16, Context::Size8192, 14.0, 4.0),
    ]);
    let found = find_launch_estimate(&model, "", Some(3000)).unwrap();
    assert_eq!(found.context, Context::Size4096);
}

#[test]
fn find_launch_estimate_falls_back_to_the_largest_context_when_none_are_big_enough() {
    let model = model_with_estimates(vec![
        estimate(Quant::FP16, Context::Size2048, 14.0, 1.0),
        estimate(Quant::FP16, Context::Size4096, 14.0, 2.0),
    ]);
    let found = find_launch_estimate(&model, "", Some(999_999)).unwrap();
    assert_eq!(found.context, Context::Size4096);
}

#[test]
fn find_launch_estimate_without_a_context_takes_the_first_matching_candidate() {
    let model = model_with_estimates(vec![
        estimate(Quant::FP16, Context::Size2048, 14.0, 1.0),
        estimate(Quant::FP16, Context::Size4096, 14.0, 2.0),
    ]);
    let found = find_launch_estimate(&model, "", None).unwrap();
    assert_eq!(found.context, Context::Size2048);
}

#[test]
fn find_launch_estimate_is_none_for_a_model_with_no_estimates() {
    let model = model_with_estimates(vec![]);
    assert!(find_launch_estimate(&model, "", None).is_none());
}

// ---------------------------------------------------------------------------
// launch_gpu_util
// ---------------------------------------------------------------------------

fn query(gpu_util: Option<&str>, recalc: Option<bool>) -> LaunchModalQuery {
    LaunchModalQuery {
        model: None,
        host: None,
        port: None,
        namespace: None,
        quantization: None,
        dtype: None,
        limit_mm_per_prompt: None,
        max_model_len: None,
        gpu_memory_utilization: gpu_util.map(str::to_string),
        prefix_caching: None,
        task: None,
        enable_tool_calling: None,
        recalculate_gpu_util: recalc,
    }
}

#[test]
fn launch_gpu_util_uses_the_explicit_query_value_unless_recalculating() {
    let model = model_with_estimates(vec![estimate(Quant::FP16, Context::Size4096, 14.0, 2.0)]);
    let q = query(Some("0.5"), None);
    let value = launch_gpu_util(Some(&model), "", None, &q, &gpu(24.0, 20.0));
    assert_eq!(value, 0.5);
}

#[test]
fn launch_gpu_util_recalculates_from_the_estimate_when_asked() {
    let model = model_with_estimates(vec![estimate(Quant::FP16, Context::Size4096, 14.0, 2.0)]);
    let q = query(Some("0.5"), Some(true));
    let value = launch_gpu_util(Some(&model), "", None, &q, &gpu(24.0, 20.0));
    assert_eq!(value, round_gpu_util_up((14.0 + 1.5) / (24.0 - 2.0)) as f32);
}

#[test]
fn launch_gpu_util_defaults_to_0_90_with_no_model_and_no_query_value() {
    let q = query(None, None);
    let value = launch_gpu_util(None, "", None, &q, &gpu(24.0, 20.0));
    assert_eq!(value, 0.90);
}

#[test]
fn launch_gpu_util_defaults_to_0_90_when_the_model_has_no_matching_estimate() {
    let model = model_with_estimates(vec![]);
    let q = query(None, None);
    let value = launch_gpu_util(Some(&model), "", None, &q, &gpu(24.0, 20.0));
    assert_eq!(value, 0.90);
}

// ---------------------------------------------------------------------------
// launch_fit_note
// ---------------------------------------------------------------------------

#[test]
fn launch_fit_note_warns_when_no_model_is_selected() {
    let (class, _, i18n, args, disabled) = launch_fit_note(None, "", None, 0.9, &gpu(24.0, 20.0));
    assert_eq!(class, "fit-line fit-warn");
    assert_eq!(i18n, "ui_vllm_fit_select_model");
    assert!(args.is_none());
    assert!(disabled);
}

#[test]
fn launch_fit_note_warns_when_no_estimate_matches() {
    let model = model_with_estimates(vec![]);
    let (class, _, i18n, _, disabled) =
        launch_fit_note(Some(&model), "", None, 0.9, &gpu(24.0, 20.0));
    assert_eq!(class, "fit-line fit-warn");
    assert_eq!(i18n, "ui_vllm_fit_no_estimate");
    assert!(disabled);
}

#[test]
fn launch_fit_note_reports_wont_fit_when_over_budget() {
    // 14 + (2 * 0.5) = 15 GB needed, budget is 24 * 0.5 = 12 GB -> over budget.
    let model = model_with_estimates(vec![estimate(Quant::FP16, Context::Size4096, 14.0, 2.0)]);
    let (class, _, i18n, args, disabled) =
        launch_fit_note(Some(&model), "", None, 0.5, &gpu(24.0, 20.0));
    assert_eq!(class, "fit-line fit-no");
    assert_eq!(i18n, "ui_vllm_fit_wont_fit_budget");
    assert!(args.is_some());
    assert!(disabled);
}

#[test]
fn launch_fit_note_reports_wont_fit_when_not_enough_free_vram_right_now() {
    // Budget is huge (0.95 * 24 ~= 22.8) so it clears the budget check, but
    // free VRAM is tiny.
    let model = model_with_estimates(vec![estimate(Quant::FP16, Context::Size4096, 5.0, 1.0)]);
    let (class, _, i18n, _, disabled) =
        launch_fit_note(Some(&model), "", None, 0.95, &gpu(24.0, 1.0));
    assert_eq!(class, "fit-line fit-no");
    assert_eq!(i18n, "ui_vllm_fit_wont_fit_free");
    assert!(disabled);
}

#[test]
fn launch_fit_note_warns_on_a_tight_fit_but_does_not_disable_launch() {
    // required ~= max(5 + 1*0.95, 24*0.95) = 22.8; free = 24 -> remaining ~1.2 < 2.0.
    let model = model_with_estimates(vec![estimate(Quant::FP16, Context::Size4096, 5.0, 1.0)]);
    let (class, _, i18n, _, disabled) =
        launch_fit_note(Some(&model), "", None, 0.95, &gpu(24.0, 24.0));
    assert_eq!(class, "fit-line fit-warn");
    assert_eq!(i18n, "ui_vllm_fit_tight");
    assert!(!disabled);
}

#[test]
fn launch_fit_note_reports_ok_with_comfortable_headroom() {
    let model = model_with_estimates(vec![estimate(Quant::FP16, Context::Size4096, 5.0, 1.0)]);
    let (class, _, i18n, _, disabled) =
        launch_fit_note(Some(&model), "", None, 0.5, &gpu(24.0, 24.0));
    assert_eq!(class, "fit-line fit-ok");
    assert_eq!(i18n, "ui_vllm_fit_ok");
    assert!(!disabled);
}

// ---------------------------------------------------------------------------
// fit_note_icon_class / get_vllm_namespace
// ---------------------------------------------------------------------------

#[test]
fn fit_note_icon_class_maps_each_fit_class_to_its_icon() {
    assert_eq!(
        fit_note_icon_class("fit-line fit-no"),
        "fa-solid fa-circle-xmark"
    );
    assert_eq!(
        fit_note_icon_class("fit-line fit-warn"),
        "fa-solid fa-triangle-exclamation"
    );
    assert_eq!(
        fit_note_icon_class("fit-line fit-ok"),
        "fa-solid fa-circle-check"
    );
}

#[actix_web::test]
async fn get_vllm_namespace_reflects_mock_and_native_management_modes() {
    // VLLM_MANAGEMENT_MODE is process-global; this crate's env_support lock
    // (shared with every other test here that touches the same var) keeps
    // this deterministic under the default parallel test runner.
    use crate::env_support::env_lock;
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("VLLM_MANAGEMENT_MODE", "mock") };
    assert_eq!(get_vllm_namespace(), "mock");

    unsafe { std::env::set_var("VLLM_MANAGEMENT_MODE", "native") };
    assert_eq!(get_vllm_namespace(), "native");

    unsafe { std::env::remove_var("VLLM_MANAGEMENT_MODE") };
}

// ---------------------------------------------------------------------------
// render_launch_modal / render_stop_modal - light structural assertions
// ---------------------------------------------------------------------------

#[test]
fn render_launch_modal_includes_the_model_select_and_fit_note() {
    let model = model_with_estimates(vec![estimate(Quant::FP16, Context::Size4096, 5.0, 1.0)]);
    let q = query(None, None);
    let html = render_launch_modal(vec![model], &q, &gpu(24.0, 24.0));
    assert!(html.contains("launch-instance-modal"));
    assert!(html.contains("test-model"));
    assert!(html.contains("fit-line"));
}

#[test]
fn render_stop_modal_includes_the_instance_id_and_optional_model_name() {
    let html = render_stop_modal("inst-1", Some("llama-3-8b"));
    assert!(html.contains("stop-instance-modal") || html.contains("inst-1"));
    assert!(html.contains("llama-3-8b"));

    let html_no_model = render_stop_modal("inst-2", None);
    assert!(html_no_model.contains("inst-2"));
}

// ---------------------------------------------------------------------------
// Handlers - light smoke coverage via actix test
// ---------------------------------------------------------------------------

async fn ensure_store() {
    static ONCE: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    ONCE.get_or_init(|| async {
        let db = quench_db::prelude::Db::connect("")
            .await
            .expect("in-memory database");
        init_model_store(db).await;
    })
    .await;
}

#[actix_web::test]
async fn handle_launch_modal_renders_ok_with_a_model_in_the_store() {
    ensure_store().await;
    let model = model_with_estimates(vec![estimate(Quant::FP16, Context::Size4096, 5.0, 1.0)]);
    get_store().insert_model(&model).await;

    let app = actix_test::init_service(App::new().service(handle_launch_modal)).await;
    let req = actix_test::TestRequest::get()
        .uri(&format!("/launch-modal?model={}", model.name))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("launch-instance-modal"));
}

#[actix_web::test]
async fn empty_launch_modal_renders_the_shell() {
    let app = actix_test::init_service(App::new().service(empty_launch_modal)).await;
    let req = actix_test::TestRequest::get()
        .uri("/launch-modal/empty")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn handle_stop_modal_renders_the_provided_id_and_model() {
    let app = actix_test::init_service(App::new().service(handle_stop_modal)).await;
    let req = actix_test::TestRequest::get()
        .uri("/stop-modal?id=abc&model=llama")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("llama"));
}

#[actix_web::test]
async fn empty_stop_modal_renders_the_shell() {
    let app = actix_test::init_service(App::new().service(empty_stop_modal)).await;
    let req = actix_test::TestRequest::get()
        .uri("/stop-modal/empty")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}
