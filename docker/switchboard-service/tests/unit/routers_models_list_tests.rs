//! Direct coverage of `routers/models/list.rs`'s `apply_filters` (pure, and
//! already `pub`) plus HTTP-level coverage of the handlers that render
//! model cards/estimate grids, which exercises the private render helpers
//! transitively - the same approach `routers_models_handlers_tests.rs` uses.

use crate::env_support::{env_lock, store_lock};
use actix_web::http::StatusCode;
use actix_web::web::Data;
use actix_web::{App, test as actix_test};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use switchboard_service::routers::gpu::monitor::GpuInfo;
use switchboard_service::routers::models;
use switchboard_service::routers::models::list::apply_filters;
use switchboard_service::routers::models::store::{get_store, init_model_store};
use switchboard_service::routers::models::types::{
    Context, Model, ModelEstimate, ModelFilters, Quant,
};
use switchboard_service::routers::vllm::mock::MockVllmEngine;
use tokio::sync::OnceCell as AsyncOnceCell;

async fn ensure_store() {
    static ONCE: AsyncOnceCell<()> = AsyncOnceCell::const_new();
    ONCE.get_or_init(|| async {
        let db = Db::connect("").await.expect("in-memory database");
        init_model_store(db).await;
    })
    .await;
}

fn engine() -> std::sync::Arc<dyn switchboard_service::routers::vllm::engine::VllmEngine> {
    std::sync::Arc::new(MockVllmEngine)
}

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

fn filters() -> ModelFilters {
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

#[test]
fn apply_filters_defaults_the_source_to_hf() {
    let mut models = vec![
        model("a", "HF", Quant::FP16, Context::Size4096, 7.0),
        model("b", "GGUF", Quant::FP16, Context::Size4096, 7.0),
    ];
    apply_filters(&mut models, &filters(), &gpu());
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "a");
}

#[test]
fn apply_filters_matches_source_case_insensitively() {
    let mut models = vec![model("a", "GGUF", Quant::FP16, Context::Size4096, 7.0)];
    let mut f = filters();
    f.source = Some("gguf".to_string());
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);
}

#[test]
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

#[test]
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

#[test]
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

#[test]
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

#[test]
fn apply_filters_by_context_as_number_and_string() {
    let mut models = vec![
        model("small", "HF", Quant::FP16, Context::Size4096, 7.0),
        model("big", "HF", Quant::FP16, Context::Size32768, 7.0),
    ];
    let mut f = filters();
    f.context = Some(serde_json::json!(8192));
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "big");

    let mut models = vec![
        model("small", "HF", Quant::FP16, Context::Size4096, 7.0),
        model("big", "HF", Quant::FP16, Context::Size32768, 7.0),
    ];
    let mut f = filters();
    f.context = Some(serde_json::json!("8192"));
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "big");
}

#[test]
fn apply_filters_by_context_zero_or_unparseable_keeps_everything() {
    let mut models = vec![model("a", "HF", Quant::FP16, Context::Size4096, 7.0)];
    let mut f = filters();
    f.context = Some(serde_json::json!(0));
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);

    let mut models = vec![model("a", "HF", Quant::FP16, Context::Size4096, 7.0)];
    let mut f = filters();
    f.context = Some(serde_json::json!(true));
    apply_filters(&mut models, &f, &gpu());
    assert_eq!(models.len(), 1);
}

#[test]
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

#[test]
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

#[test]
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

#[test]
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

#[test]
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

#[actix_web::test]
async fn handle_grid_shows_a_fit_line_when_an_estimate_fits_with_a_small_margin() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    let _store_guard = store_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    // gpu().free_gb is 20.0; a total_gb of 19.0 leaves a 1.0 GB margin, which
    // is `<= 2.0` and hits the "fit-warn" branch rather than "fit-ok".
    let mut tight = model_with_estimate("tight-fit", 19.0);
    tight.path = "/tmp/list-test/handle-grid-tight".to_string();
    get_store().insert_model(&tight).await;

    // No estimates at all hits `render_model_grid`'s third branch
    // ("No estimates").
    let mut bare = model("no-estimates", "HF", Quant::FP16, Context::Size4096, 7.0);
    bare.path = "/tmp/list-test/handle-grid-bare".to_string();
    get_store().insert_model(&bare).await;

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .service(models::scope(engine(), JwtConfig::for_tests())),
    )
    .await;

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/models/grid")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8 html");
    assert!(html.contains("tight-fit"));
    assert!(html.contains("no-estimates"));
    assert!(html.contains("fit-warn") || html.contains("fit-no"));
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn handle_grid_shows_the_delete_button_when_the_caller_can_delete_models() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    let _store_guard = store_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let mut deletable = model("deletable", "HF", Quant::FP16, Context::Size4096, 7.0);
    deletable.path = "/tmp/list-test/handle-grid-deletable".to_string();
    get_store().insert_model(&deletable).await;

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .service(models::scope(engine(), JwtConfig::for_tests())),
    )
    .await;

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/models/grid")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8 html");
    assert!(html.contains("card-delete"));
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn estimates_modal_filters_by_fit_context_and_quant() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    let _store_guard = store_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let mut with_estimates = model("multi-estimate", "HF", Quant::FP16, Context::Size4096, 7.0);
    with_estimates.path = "/tmp/list-test/estimates-modal-multi".to_string();
    with_estimates.estimates = vec![
        ModelEstimate {
            quant: Quant::FP16,
            context: Context::Size4096,
            weights_gb: 13.0,
            kv_gb: 1.0,
            total_gb: 14.0,
        },
        ModelEstimate {
            quant: Quant::Q80,
            context: Context::Size32768,
            weights_gb: 25.0,
            kv_gb: 5.0,
            total_gb: 30.0,
        },
    ];
    get_store().insert_model(&with_estimates).await;

    let app = actix_test::init_service(
        App::new().service(models::scope(engine(), JwtConfig::for_tests())),
    )
    .await;

    let req = actix_test::TestRequest::get()
        .uri(&format!(
            "/api/v1/models/estimates-modal?path={}&fit=fit&context=4096&quant=FP16",
            with_estimates.path
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8 html");
    assert!(html.contains("estimates-modal-content"));
    assert!(html.contains("multi-estimate"));

    // "nofit" should show the 30GB estimate (which doesn't fit in a 20GB gpu)
    // instead of the 14GB one.
    let req = actix_test::TestRequest::get()
        .uri(&format!(
            "/api/v1/models/estimates-modal?path={}&fit=nofit",
            with_estimates.path
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8 html");
    assert!(html.contains("30.0 GB"));
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}
