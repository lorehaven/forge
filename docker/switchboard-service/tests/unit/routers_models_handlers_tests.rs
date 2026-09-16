//! Handler-level tests for `routers/models/{list,delete,running,sync}.rs`.
//!
//! These all read the process-global `MODEL_STORE` (`get_store()`), which is
//! a `OnceCell` - only the first call across this whole `tests/unit.rs`
//! binary actually initializes it, every later call is a no-op that reuses
//! that instance. So every test here shares one store; each uses a path
//! unique to itself and only ever asserts on that path, which is safe under
//! the default parallel test runner even though the store itself is shared.
//!
//! Handlers take extractors as plain arguments now, so most of these call
//! the handler function directly instead of routing an HTTP request through
//! it. `EstimatesModalQuery`/`DeleteModalQuery` have private fields (they're
//! local to `list.rs`), so those two go through a real `Query::from_request`
//! over a constructed `Request` instead of a struct literal.

use crate::env_support::store_lock;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::prelude::{Form, FromRequest, Inject, Json, Query, Request};
use switchboard_service::routers::models::delete::{delete_model, delete_model_form};
use switchboard_service::routers::models::list::{
    delete_modal, empty_estimates_modal_endpoint, estimates_modal, handle_grid, handle_list,
};
use switchboard_service::routers::models::mod_impl::OptionalClaims;
use switchboard_service::routers::models::running::list_running_models;
use switchboard_service::routers::models::store::{get_store, init_model_store};
use switchboard_service::routers::models::sync::sync_models;
use switchboard_service::routers::models::types::{
    Context, DeleteModelRequest, Model, ModelFilters, Quant,
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

fn config(auth_enabled: bool) -> Inject<JwtConfig> {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    Inject(std::sync::Arc::new(config))
}

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .expect("body collects");
    String::from_utf8(collected.to_bytes().to_vec()).expect("utf8")
}

/// `EstimatesModalQuery`/`DeleteModalQuery` are private to `list.rs`, so
/// this builds a real `Request` and lets `Query::from_request` deserialize
/// through `serde` instead of constructing a struct literal.
async fn query<T: serde::de::DeserializeOwned + Send>(uri: &str) -> Query<T> {
    let container = std::sync::Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut req = Request::new(
        Method::GET,
        uri.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container,
    );
    Query::<T>::from_request(&mut req)
        .await
        .expect("query deserializes")
}

#[tokio::test]
async fn handle_list_returns_models_matching_the_stored_source() {
    ensure_store().await;
    let _store_guard = store_lock().lock().await;
    let model = sample_model("/tmp/handlers-test/handle_list");
    get_store().insert_model(&model).await;

    let filters: ModelFilters = serde_json::from_value(serde_json::json!({})).unwrap();
    let body = handle_list(Json(filters)).await.0;
    assert!(body.iter().any(|m| m.path == model.path));
}

#[tokio::test]
async fn handle_grid_renders_html_containing_the_model_name() {
    ensure_store().await;
    let _store_guard = store_lock().lock().await;
    let model = sample_model("/tmp/handlers-test/handle_grid");
    get_store().insert_model(&model).await;

    let filters: ModelFilters = serde_json::from_value(serde_json::json!({})).unwrap();
    let resp = handle_grid(OptionalClaims(None), config(false), Query(filters)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let html = body_text(resp).await;
    assert!(html.contains(&model.name));
}

#[tokio::test]
async fn estimates_modal_renders_the_empty_state_for_an_unknown_path() {
    ensure_store().await;

    let q = query("/api/v1/models/estimates-modal?path=/does/not/exist").await;
    let resp = estimates_modal(q).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let html = body_text(resp).await;
    assert!(html.contains("estimates-modal") && !html.contains("estimates-modal-content"));
}

#[tokio::test]
async fn empty_estimates_modal_endpoint_renders_the_shell() {
    ensure_store().await;

    let resp = empty_estimates_modal_endpoint().await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn delete_modal_renders_the_provided_name() {
    ensure_store().await;

    let q = query("/api/v1/models/delete-modal?path=/tmp/x&name=My%20Model").await;
    let resp = delete_modal(q).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let html = body_text(resp).await;
    assert!(html.contains("My Model"));
}

#[tokio::test]
async fn delete_model_rejects_a_path_outside_the_configured_roots() {
    ensure_store().await;

    let resp = delete_model(
        OptionalClaims(None),
        config(false),
        Json(DeleteModelRequest {
            path: "/etc/passwd".to_string(),
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_model_reports_not_found_for_a_path_that_does_not_exist_on_disk() {
    ensure_store().await;

    // Within the default HF_ROOTS fallback (`/mnt/dev/huggingface/hub`, which
    // does not exist in this sandbox), so it clears the root check but fails
    // the existence check.
    let resp = delete_model(
        OptionalClaims(None),
        config(false),
        Json(DeleteModelRequest {
            path: "/mnt/dev/huggingface/hub/nonexistent-model".to_string(),
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_model_is_forbidden_without_the_delete_model_permission() {
    ensure_store().await;

    let resp = delete_model(
        OptionalClaims(None),
        config(true),
        Json(DeleteModelRequest {
            path: "/mnt/dev/huggingface/hub/whatever".to_string(),
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_model_form_is_forbidden_without_permission() {
    ensure_store().await;

    let resp = delete_model_form(
        OptionalClaims(None),
        config(true),
        Form(DeleteModelRequest {
            path: "/mnt/dev/huggingface/hub/whatever".to_string(),
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_running_models_is_forbidden_when_auth_is_on_and_the_caller_is_not_admin() {
    let resp = list_running_models(
        OptionalClaims(None),
        config(true),
        Inject(std::sync::Arc::new(engine())),
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_running_models_returns_the_mock_engines_instances_when_admin() {
    let resp = list_running_models(
        OptionalClaims(None),
        config(false),
        Inject(std::sync::Arc::new(engine())),
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn list_running_models_maps_an_engine_error_to_500() {
    let failing: std::sync::Arc<dyn switchboard_service::routers::vllm::engine::VllmEngine> =
        std::sync::Arc::new(FailingEngine);
    let resp = list_running_models(
        OptionalClaims(None),
        config(false),
        Inject(std::sync::Arc::new(failing)),
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
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
