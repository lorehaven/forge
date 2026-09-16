//! `get_status` (REST) and `handle_sse` (SSE) under `routers/gpu`.

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::prelude::Inject;
use quench_http::request::Request;
use quench_http::response::Response;
use std::sync::Arc;
use switchboard_service::routers::gpu::{GpuBroadcaster, get_status, handle_sse, wrap_auth};

#[tokio::test]
async fn get_status_returns_gpu_info_as_json() {
    let info = get_status().await.0;
    // `GpuInfo` only derives `Serialize` (it's a broadcast payload, never
    // deserialized elsewhere in this crate), so round-trip through JSON
    // rather than asserting on struct fields directly.
    let body: serde_json::Value = serde_json::to_value(&info).unwrap();
    assert!(body.get("total_gb").and_then(|v| v.as_f64()).is_some());
    assert!(body.get("free_gb").and_then(|v| v.as_f64()).is_some());
}

#[tokio::test]
async fn gpu_sse_route_streams_a_broadcast_message() {
    use http_body_util::BodyExt;

    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let broadcaster = Inject(Arc::new(GpuBroadcaster(tx.clone())));

    let resp = handle_sse(broadcaster).await.expect("sse response");
    assert!(resp.status().is_success());

    tx.send("<div>gpu</div>".to_string()).unwrap();
    // Drop every sender so the never-ending broadcast stream actually
    // closes before collecting the body waits for end-of-stream.
    drop(tx);

    let hyper_resp = resp.into_hyper();
    assert_eq!(
        hyper_resp.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
    let collected = hyper_resp
        .into_body()
        .collect()
        .await
        .expect("stream collects cleanly");
    let text = String::from_utf8(collected.to_bytes().to_vec()).unwrap();
    assert!(text.contains("event: gpu-status"));
    assert!(text.contains("<div>gpu</div>"));
}

struct AlwaysOk;

#[async_trait]
impl Endpoint for AlwaysOk {
    async fn call(&self, _req: Request) -> Response {
        Response::new(StatusCode::OK)
    }
}

fn get(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[tokio::test]
async fn wrap_auth_lets_matching_requests_through_when_auth_is_disabled() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let app: Arc<dyn Endpoint> = Arc::new(AlwaysOk);
    let app = wrap_auth(app, JwtConfig::for_tests(), "");

    let resp = app.call(get("/api/v1/gpu/status", &container)).await;
    assert!(resp.status().is_success());
}

#[tokio::test]
async fn wrap_auth_does_not_apply_outside_its_prefix() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let app: Arc<dyn Endpoint> = Arc::new(AlwaysOk);
    let app = wrap_auth(app, config, "");

    // No token supplied, but `/health` doesn't start with `/api/v1/gpu`, so
    // `Auth` never runs against it.
    let resp = app.call(get("/health", &container)).await;
    assert!(resp.status().is_success());
}
