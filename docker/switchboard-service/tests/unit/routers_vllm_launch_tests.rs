//! `launch_instance`/`launch_instance_form` handlers and the private
//! `parse_optional_{u16,u32,f32}` helpers behind the form's string fields.

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::prelude::{Form, FromRequest, Inject, Json, Request};
use std::sync::{Arc, Mutex};
use switchboard_service::routers::models::mod_impl::OptionalClaims;
use switchboard_service::routers::vllm::engine::VllmEngine;
use switchboard_service::routers::vllm::launch::{
    launch_instance, launch_instance_form, parse_optional_f32, parse_optional_u16,
    parse_optional_u32,
};
use switchboard_service::routers::vllm::types::{LaunchRequest, VllmInstance};

#[test]
fn parse_optional_u16_parses_trims_and_ignores_blank_or_invalid() {
    assert_eq!(parse_optional_u16(Some(" 8080 ")), Some(8080));
    assert_eq!(parse_optional_u16(Some("")), None);
    assert_eq!(parse_optional_u16(Some("  ")), None);
    assert_eq!(parse_optional_u16(Some("not-a-port")), None);
    assert_eq!(parse_optional_u16(Some("99999")), None); // overflows u16
    assert_eq!(parse_optional_u16(None), None);
}

#[test]
fn parse_optional_u32_parses_trims_and_ignores_blank_or_invalid() {
    assert_eq!(parse_optional_u32(Some(" 4096 ")), Some(4096));
    assert_eq!(parse_optional_u32(Some("")), None);
    assert_eq!(parse_optional_u32(Some("nope")), None);
    assert_eq!(parse_optional_u32(None), None);
}

#[test]
fn parse_optional_f32_parses_trims_and_ignores_blank_or_invalid() {
    assert_eq!(parse_optional_f32(Some(" 0.9 ")), Some(0.9));
    assert_eq!(parse_optional_f32(Some("")), None);
    assert_eq!(parse_optional_f32(Some("nope")), None);
    assert_eq!(parse_optional_f32(None), None);
}

struct RecordingEngine {
    last_request: Arc<Mutex<Option<LaunchRequest>>>,
    fail: bool,
}

#[async_trait]
impl VllmEngine for RecordingEngine {
    async fn list_instances(&self) -> Result<Vec<VllmInstance>, String> {
        unimplemented!()
    }

    async fn launch_instance(&self, req: LaunchRequest) -> Result<VllmInstance, String> {
        if self.fail {
            return Err("engine refused to launch".to_string());
        }
        let instance = VllmInstance {
            id: "new-instance".to_string(),
            namespace: req
                .namespace
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            model: req.model.clone(),
            host: req.host.clone(),
            port: req.port,
            quantization: req.quantization.clone(),
            dtype: req.dtype.clone(),
            limit_mm_per_prompt: req.limit_mm_per_prompt.clone(),
            max_model_len: req.max_model_len,
            gpu_memory_utilization: req.gpu_memory_utilization,
            enable_prefix_caching: req.enable_prefix_caching,
            enable_tool_calling: req.enable_tool_calling,
            task: req.task.clone(),
            device: req.device.clone(),
            started_at: chrono::Utc::now(),
            status: "starting".to_string(),
            log_path: None,
            last_error: None,
        };
        *self.last_request.lock().unwrap() = Some(req);
        Ok(instance)
    }

    async fn stop_instance(&self, _id: String) -> Result<(), String> {
        unimplemented!()
    }
}

type LastRequest = Arc<Mutex<Option<LaunchRequest>>>;

fn engine(fail: bool) -> (Inject<Arc<dyn VllmEngine>>, LastRequest) {
    let last_request: LastRequest = Arc::new(Mutex::new(None));
    let engine: Arc<dyn VllmEngine> = Arc::new(RecordingEngine {
        last_request: last_request.clone(),
        fail,
    });
    (Inject(Arc::new(engine)), last_request)
}

fn config(auth_enabled: bool) -> Inject<JwtConfig> {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    Inject(Arc::new(config))
}

fn launch_request_json(value: serde_json::Value) -> Json<LaunchRequest> {
    Json(serde_json::from_value(value).expect("valid LaunchRequest json"))
}

/// `LaunchRequestForm` is private to `launch.rs`, so this goes through a
/// real `Form::from_request` over a constructed `Request` - the same
/// urlencoded-body parsing a real request would go through.
async fn launch_request_form(
    pairs: &[(&str, &str)],
) -> Form<switchboard_service::routers::vllm::launch::LaunchRequestForm> {
    let encoded = serde_urlencoded::to_string(pairs).unwrap();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut req = Request::new(
        Method::POST,
        "/api/v1/vllm/instances/form".parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::from(encoded)),
        container,
    );
    Form::from_request(&mut req)
        .await
        .expect("form deserializes")
}

async fn body_json<T: serde::de::DeserializeOwned>(resp: quench_http::response::Response) -> T {
    let collected = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .expect("body collects");
    serde_json::from_slice(&collected.to_bytes()).expect("valid json body")
}

#[tokio::test]
async fn launch_instance_is_forbidden_without_the_launch_permission() {
    let (engine, _) = engine(false);
    let req = launch_request_json(serde_json::json!({
        "model": "llama-3-8b", "host": "0.0.0.0", "port": 8000,
        "enable_prefix_caching": false
    }));

    let resp = launch_instance(OptionalClaims(None), config(true), req, engine).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn launch_instance_rejects_an_empty_model_name() {
    let (engine, _) = engine(false);
    let req = launch_request_json(serde_json::json!({
        "model": "   ", "host": "0.0.0.0", "port": 8000,
        "enable_prefix_caching": false
    }));

    let resp = launch_instance(OptionalClaims(None), config(false), req, engine).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn launch_instance_succeeds_and_returns_the_instance_as_json() {
    let (engine, last_request) = engine(false);
    let req = launch_request_json(serde_json::json!({
        "model": "llama-3-8b", "host": "0.0.0.0", "port": 8000,
        "enable_prefix_caching": true
    }));

    let resp = launch_instance(OptionalClaims(None), config(false), req, engine).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    let body: VllmInstance = body_json(resp).await;
    assert_eq!(body.model, "llama-3-8b");
    assert!(last_request.lock().unwrap().is_some());
}

#[tokio::test]
async fn launch_instance_maps_an_engine_error_to_500() {
    let (engine, _) = engine(true);
    let req = launch_request_json(serde_json::json!({
        "model": "llama-3-8b", "host": "0.0.0.0", "port": 8000,
        "enable_prefix_caching": false
    }));

    let resp = launch_instance(OptionalClaims(None), config(false), req, engine).await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn launch_instance_form_maps_every_field_and_defaults_the_rest() {
    let (engine, last_request) = engine(false);
    let form = launch_request_form(&[
        ("model", "llama-3-8b"),
        ("namespace", "  "),
        ("port", "not-a-number"),
        ("max_model_len", "4096"),
        ("prefix_caching", "true"),
    ])
    .await;

    let resp = launch_instance_form(OptionalClaims(None), config(false), form, engine).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    let sent = last_request
        .lock()
        .unwrap()
        .take()
        .expect("request recorded");
    assert_eq!(sent.model, "llama-3-8b");
    assert_eq!(sent.host, "0.0.0.0"); // default
    assert_eq!(sent.port, 8000); // "not-a-number" falls back to the default
    assert_eq!(sent.namespace, None); // blank -> None
    assert_eq!(sent.max_model_len, Some(4096));
    assert!(sent.enable_prefix_caching); // via the legacy `prefix_caching` alias
    assert_eq!(sent.gpu_memory_utilization, Some(0.90)); // default
}

#[tokio::test]
async fn launch_instance_form_is_forbidden_without_the_launch_permission() {
    let (engine, _) = engine(false);
    let form = launch_request_form(&[("model", "llama-3-8b")]).await;

    let resp = launch_instance_form(OptionalClaims(None), config(true), form, engine).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
