//! HTTP-level tests for `routers/ui/pages/auth.rs`'s `auth_status` and
//! `routers/ui/pages/jobs.rs`'s log-viewer fragment. Both handlers are
//! `pub(super)`, reachable only through the mounted routes, not directly.
//! `login`/`login_slash`/`callback` need a registered `SsoConfig` this test
//! app doesn't provide, so they're not exercised here.
//!
//! `tests/unit.rs` is a separate test binary from `tests/integration.rs`
//! (no shared `support` module), so this file builds its own minimal
//! `discover_and_mount` + hand-built `Request` helpers.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use std::sync::Arc;

async fn app(config: JwtConfig) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    conveyor_service::routers::ui::register_routes();
    let container = ContainerBuilder::new()
        .provide(config)
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn req(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

async fn json_body(resp: quench_http::response::Response) -> serde_json::Value {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    serde_json::from_slice(&collected.to_bytes()).expect("valid json body")
}

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    String::from_utf8_lossy(&collected.to_bytes()).into_owned()
}

#[tokio::test]
async fn auth_status_reports_the_dev_admin_bypass_when_auth_is_disabled() {
    let config = JwtConfig::for_tests();
    assert!(!config.auth_enabled);
    let (app, container) = app(config).await;

    let resp = app.call(req("/ui/status", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["username"], "dev");
    assert_eq!(body["roles"][0], "admin");
}

#[tokio::test]
async fn auth_status_is_anonymous_when_auth_is_enabled_and_there_is_no_session() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let (app, container) = app(config).await;

    let resp = app.call(req("/ui/status", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["authenticated"], false);
    assert!(body["username"].is_null());
    assert!(body["roles"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn job_log_viewer_connects_to_the_streams_own_sse_endpoint() {
    let config = JwtConfig::for_tests();
    let (app, container) = app(config).await;

    let resp = app.call(req("/ui/jobs/job-42/log", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let html = body_text(resp).await;
    assert!(html.contains(r#"id="log-job-42""#));
    assert!(html.contains("jobs/job-42/stream"));
    assert!(html.contains("sse-connect"));
}

#[tokio::test]
async fn job_log_viewer_offers_a_raw_view_and_a_copy_button() {
    let config = JwtConfig::for_tests();
    let (app, container) = app(config).await;

    let resp = app.call(req("/ui/jobs/job-42/log", &container)).await;
    let html = body_text(resp).await;

    // "Open raw" points straight at the API's own raw endpoint, in a new tab.
    assert!(html.contains("jobs/job-42/raw"));
    assert!(html.contains(r#"target="_blank""#));
    assert!(html.contains(r#"rel="noopener""#));

    // Copy reads the log element by the same id it was just asserted to have.
    assert!(html.contains("navigator.clipboard.writeText"));
    assert!(html.contains("log-job-42"));
}

#[tokio::test]
async fn job_log_viewer_redirects_to_login_when_auth_is_enabled_and_there_is_no_session() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let (app, container) = app(config).await;

    let resp = app.call(req("/ui/jobs/job-42/log", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}
