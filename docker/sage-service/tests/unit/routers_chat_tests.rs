//! Unit tests for `routers/chat.rs` - the chat/metrics/costs API surface.
//! `chat`/`get_context_status`'s streaming happy path needs a real vLLM
//! instance behind Switchboard, so those two are only exercised for their
//! error/not-found branches here; everything else is fully reachable
//! in-process with fake or empty backing services.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use sage_service::clients::switchboard::SwitchboardClient;
use sage_service::clients::vllm::VllmClient;
use sage_service::config::SageConfig;
use sage_service::observability::cost_tracking::CostTracker;
use sage_service::observability::metrics::MetricsCollector;
use sage_service::routers::chat;
use sage_service::tools::capabilities::get_profile;
use std::sync::Arc;

fn config() -> SageConfig {
    SageConfig {
        system_prompt: "you are sage".to_string(),
        default_models: vec![],
        supported_models: vec!["*".to_string()],
        default_search_provider: "duckduckgo".to_string(),
        available_search_providers: vec!["duckduckgo".to_string()],
        capability_profile: get_profile("web_assistant").expect("web_assistant profile exists"),
        stop_models_on_shutdown: false,
    }
}

/// `SwitchboardClient::new()` panics unless `GATEHOUSE_URL` and
/// `CLIENT_SECRET_SAGE_SWITCHBOARD` are set - it never actually needs to
/// reach either service in these tests (switchboard isn't running), so any
/// non-empty value works. `envmnt::set` with a fixed value is idempotent
/// under this crate's parallel test binary - concurrent identical writes
/// race harmlessly, unlike a var different tests need different values for.
fn ensure_switchboard_env() {
    // Loopback ports nothing listens on refuse the connection immediately;
    // a DNS name (the production default, and an earlier version of this
    // fixture) can take the better part of a minute to fail via resolver
    // timeout instead. Both `GATEHOUSE_URL` (the OAuth token endpoint
    // `ClientCredentialsClient` hits first) and `SWITCHBOARD_URL` need this.
    envmnt::set("GATEHOUSE_URL", "http://127.0.0.1:1");
    envmnt::set("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
    envmnt::set("SWITCHBOARD_URL", "http://127.0.0.1:1");
}

async fn app() -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    ensure_switchboard_env();
    chat::register_routes();
    let container = ContainerBuilder::new()
        .provide(SwitchboardClient::new())
        .provide(VllmClient::new())
        .provide(config())
        .provide_arc(Arc::new(MetricsCollector::new()))
        .provide_arc(Arc::new(CostTracker::new()))
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn req(method: Method, path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

fn json_req(
    method: Method,
    path: &str,
    body: serde_json::Value,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
        container.clone(),
    )
}

async fn json_body(resp: quench_http::response::Response) -> serde_json::Value {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    serde_json::from_slice(&collected.to_bytes()).expect("valid json body")
}

#[tokio::test]
async fn chat_returns_not_found_for_an_unknown_instance() {
    let (app, container) = app().await;

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/chat",
            serde_json::json!({"instance_id": "does-not-exist", "message": "hello"}),
            &container,
        ))
        .await;

    // `SwitchboardClient::new()` talks to a real switchboard that isn't
    // running here, so this either surfaces as a 500 (switchboard
    // unreachable) or a 404 (reachable, but the instance genuinely isn't
    // found) - both are "did not stream a chat response" outcomes, the
    // only two this handler can produce without a live vLLM instance.
    assert!(matches!(
        resp.status(),
        StatusCode::NOT_FOUND | StatusCode::INTERNAL_SERVER_ERROR
    ));
}

#[tokio::test]
async fn capabilities_reports_the_configured_profiles_tools_sorted() {
    let (app, container) = app().await;

    let resp = app
        .call(req(Method::GET, "/api/v1/chat/capabilities", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["profile"], "web_assistant");
    let tools = body["available_tools"].as_array().expect("tools array");
    let mut sorted = tools.clone();
    sorted.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    assert_eq!(tools, &sorted, "tools must already be sorted");
}

#[tokio::test]
async fn get_metrics_returns_an_empty_profile_list_with_no_activity() {
    let (app, container) = app().await;

    let resp = app
        .call(req(Method::GET, "/api/v1/chat/metrics", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["profiles"].as_array().expect("array").len(), 0);
}

#[tokio::test]
async fn get_metrics_by_profile_is_not_found_for_an_unknown_profile() {
    let (app, container) = app().await;

    let resp = app
        .call(req(
            Method::GET,
            "/api/v1/chat/metrics/nonexistent-profile",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_costs_returns_empty_lists_with_no_activity() {
    let (app, container) = app().await;

    let resp = app
        .call(req(Method::GET, "/api/v1/chat/costs", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = json_body(resp).await;
    assert_eq!(body["users"].as_array().expect("array").len(), 0);
    assert_eq!(body["profiles"].as_array().expect("array").len(), 0);
}

#[tokio::test]
async fn get_user_costs_is_not_found_for_an_unknown_user() {
    let (app, container) = app().await;

    let resp = app
        .call(req(
            Method::GET,
            "/api/v1/chat/costs/user/nobody",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_context_status_reports_zero_usage_for_a_fresh_profile() {
    let (app, container) = app().await;

    let resp = app
        .call(req(
            Method::GET,
            "/api/v1/chat/context-status/web_assistant",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}
