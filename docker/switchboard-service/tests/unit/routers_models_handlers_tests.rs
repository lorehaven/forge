//! HTTP-handler-level tests for `routers/models/{list,delete,sync}.rs`.
//!
//! These all read the process-global `MODEL_STORE` (`get_store()`), which is
//! a `OnceCell` - only the first call across this whole `tests/unit.rs`
//! binary actually initializes it, every later call is a no-op that reuses
//! that instance. So every test here shares one store; each uses a path
//! unique to itself and only ever asserts on that path, which is safe under
//! the default parallel test runner even though the store itself is shared.

use crate::env_support::{env_lock, store_lock};
use actix_web::http::StatusCode;
use actix_web::web::Data;
use actix_web::{App, test};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use switchboard_service::routers::models;
use switchboard_service::routers::models::store::{get_store, init_model_store};
use switchboard_service::routers::models::sync::sync_models;
use switchboard_service::routers::models::types::{Context, Model, Quant};
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

fn sample_model(path: &str) -> Model {
    Model {
        source: "HF".to_string(),
        name: format!("handler test model {path}"),
        path: path.to_string(),
        architecture: None,
        vllm_supported: false,
        quant: Quant::FP16,
        context: Context::Size4096,
        layers: 32,
        hidden_size: 4096,
        params_billion: 7.0,
        estimates: vec![],
    }
}

fn engine() -> std::sync::Arc<dyn switchboard_service::routers::vllm::engine::VllmEngine> {
    std::sync::Arc::new(MockVllmEngine)
}

/// A `VllmEngine` whose `list_instances` always errors, for exercising
/// `list_running_models`'s 500 path - `MockVllmEngine` never errors.
struct FailingEngine;

#[async_trait::async_trait]
impl switchboard_service::routers::vllm::engine::VllmEngine for FailingEngine {
    async fn list_instances(
        &self,
    ) -> Result<Vec<switchboard_service::routers::vllm::types::VllmInstance>, String> {
        Err("backend unreachable".to_string())
    }

    async fn launch_instance(
        &self,
        _req: switchboard_service::routers::vllm::types::LaunchRequest,
    ) -> Result<switchboard_service::routers::vllm::types::VllmInstance, String> {
        unimplemented!("not exercised by this test")
    }

    async fn stop_instance(&self, _id: String) -> Result<(), String> {
        unimplemented!("not exercised by this test")
    }
}

#[actix_web::test]
async fn handle_list_returns_models_matching_the_stored_source() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    let _store_guard = store_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };
    let model = sample_model("/tmp/handlers-test/handle_list");
    get_store().insert_model(&model).await;

    let app =
        test::init_service(App::new().service(models::scope(engine(), JwtConfig::for_tests())))
            .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/models/list")
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Vec<Model> = test::read_body_json(resp).await;
    assert!(body.iter().any(|m| m.path == model.path));
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn handle_grid_renders_html_containing_the_model_name() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    let _store_guard = store_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };
    let model = sample_model("/tmp/handlers-test/handle_grid");
    get_store().insert_model(&model).await;

    // `handle_grid` (unlike `handle_list`) extracts `web::Data<JwtConfig>`
    // directly, which in production comes from `base_path_scope`'s top-level
    // `.app_data(jwt_config.clone())` rather than from `models::scope`
    // itself - so an isolated test of just `models::scope` has to provide it.
    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .service(models::scope(engine(), JwtConfig::for_tests())),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/models/grid")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8 html");
    assert!(html.contains(&model.name));
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn estimates_modal_renders_the_empty_state_for_an_unknown_path() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app =
        test::init_service(App::new().service(models::scope(engine(), JwtConfig::for_tests())))
            .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/models/estimates-modal?path=/does/not/exist")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8 html");
    assert!(html.contains("estimates-modal") && !html.contains("estimates-modal-content"));
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn empty_estimates_modal_endpoint_renders_the_shell() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app =
        test::init_service(App::new().service(models::scope(engine(), JwtConfig::for_tests())))
            .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/models/estimates-modal/empty")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn delete_modal_renders_the_provided_name() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app =
        test::init_service(App::new().service(models::scope(engine(), JwtConfig::for_tests())))
            .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/models/delete-modal?path=/tmp/x&name=My%20Model")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8 html");
    assert!(html.contains("My Model"));
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn delete_model_rejects_a_path_outside_the_configured_roots() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .service(models::delete::delete_model),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/delete")
        .set_json(serde_json::json!({ "path": "/etc/passwd" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn delete_model_reports_not_found_for_a_path_that_does_not_exist_on_disk() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .service(models::delete::delete_model),
    )
    .await;

    // Within the default HF_ROOTS fallback (`/mnt/dev/huggingface/hub`, which
    // does not exist in this sandbox), so it clears the root check but fails
    // the existence check.
    let req = test::TestRequest::post()
        .uri("/delete")
        .set_json(serde_json::json!({ "path": "/mnt/dev/huggingface/hub/nonexistent-model" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn delete_model_is_forbidden_without_the_delete_model_permission() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .service(models::delete::delete_model),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/delete")
        .set_json(serde_json::json!({ "path": "/mnt/dev/huggingface/hub/whatever" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn delete_model_form_is_forbidden_without_permission() {
    ensure_store().await;
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .service(models::delete::delete_model_form),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/delete-form")
        .set_form([("path", "/mnt/dev/huggingface/hub/whatever")])
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[tokio::test]
async fn list_running_models_is_forbidden_when_auth_is_on_and_the_caller_is_not_admin() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .app_data(Data::new(engine()))
            .service(models::running::list_running_models),
    )
    .await;

    let req = test::TestRequest::get().uri("/running").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn list_running_models_returns_the_mock_engines_instances_when_admin() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .app_data(Data::new(engine()))
            .service(models::running::list_running_models),
    )
    .await;

    let req = test::TestRequest::get().uri("/running").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn list_running_models_maps_an_engine_error_to_500() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let failing: std::sync::Arc<dyn switchboard_service::routers::vllm::engine::VllmEngine> =
        std::sync::Arc::new(FailingEngine);
    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests()))
            .app_data(Data::new(failing))
            .service(models::running::list_running_models),
    )
    .await;

    let req = test::TestRequest::get().uri("/running").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[tokio::test]
async fn sync_models_removes_stale_entries_not_present_on_disk() {
    ensure_store().await;
    // This wipes every model currently in the shared store (see
    // `store_lock`'s docs), so it must not interleave with any other test
    // that expects its own inserted model to still be there afterward.
    let _store_guard = store_lock().lock().await;
    let stale_path = "/tmp/handlers-test/sync-stale-entry";
    get_store().insert_model(&sample_model(stale_path)).await;
    assert!(get_store().get_model(stale_path).await.is_some());

    // Neither HF_ROOTS nor GGUF_ROOTS exist on disk in this sandbox, so
    // `get_on_disk_model_paths` is empty and every stored model - including
    // the one just inserted - counts as stale and gets removed.
    sync_models().await;

    assert!(get_store().get_model(stale_path).await.is_none());
}
