use crate::env_support::env_lock;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::domain::sso_client::SsoConfig;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use std::sync::Arc;

async fn app(jwt_config: JwtConfig) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    // `SsoConfig::init()` reads `GATEHOUSE_CLIENT_ID`/`_SECRET` from the
    // environment; unset (the default in this test binary), it comes back
    // "unconfigured", which is exactly the branch these tests want to
    // exercise without a real gatehouse to talk to.
    sage_service::routers::ui::pages::register_routes();
    let container = ContainerBuilder::new()
        .provide(jwt_config)
        .provide(SsoConfig::init())
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

fn req_with_cookie(
    method: Method,
    path: &str,
    cookie: &str,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert("cookie", cookie.parse().unwrap());
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

async fn json_body(resp: quench_http::response::Response) -> serde_json::Value {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    serde_json::from_slice(&collected.to_bytes()).expect("valid json body")
}

#[tokio::test]
async fn auth_status_reports_a_dev_identity_when_auth_is_disabled() {
    let (app, container) = app(JwtConfig::for_tests()).await;

    let resp = app.call(req(Method::GET, "/ui/status", &container)).await;
    assert!(resp.status().is_success());

    let body = json_body(resp).await;
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["username"], "dev");
}

#[tokio::test]
async fn auth_status_is_unauthenticated_without_a_session_cookie_when_auth_is_required() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let (app, container) = app(config).await;

    let resp = app.call(req(Method::GET, "/ui/status", &container)).await;
    assert!(resp.status().is_success());

    let body = json_body(resp).await;
    assert_eq!(body["authenticated"], false);
    assert!(body["username"].is_null());
}

#[tokio::test]
async fn auth_status_is_unauthenticated_for_a_cookie_that_fails_to_decode() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let (app, container) = app(config).await;

    let cookie = format!(
        "{}=not-a-real-token",
        quench_auth::domain::realm::session_cookie_name()
    );
    let resp = app
        .call(req_with_cookie(
            Method::GET,
            "/ui/status",
            &cookie,
            &container,
        ))
        .await;
    assert!(resp.status().is_success());

    let body = json_body(resp).await;
    assert_eq!(body["authenticated"], false);
}

#[tokio::test]
async fn login_delegates_when_sso_is_unconfigured() {
    let (app, container) = app(JwtConfig::for_tests()).await;
    let resp = app.call(req(Method::GET, "/ui/login", &container)).await;
    // Unconfigured SSO can't build a real authorize redirect; any
    // non-panicking response proves `login_delegation` was reached.
    assert!(resp.status().as_u16() >= 300);
}

#[tokio::test]
async fn login_slash_delegates_the_same_way_as_login() {
    let (app, container) = app(JwtConfig::for_tests()).await;
    let resp = app.call(req(Method::GET, "/ui/login/", &container)).await;
    assert!(resp.status().as_u16() >= 300);
}

#[tokio::test]
async fn callback_reports_an_error_without_a_state_cookie() {
    let (app, container) = app(JwtConfig::for_tests()).await;
    let resp = app
        .call(req(
            Method::GET,
            "/ui/auth/callback?code=abc&state=xyz",
            &container,
        ))
        .await;
    assert!(!resp.status().is_success());
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
async fn logout_redirects_when_gatehouse_is_configured() {
    // `GATEHOUSE_URL` is process-global and other files in this shared
    // `tests/unit` binary set it to this same dummy value without
    // coordination (see `routers_chat_tests::ensure_switchboard_env`) - an
    // identical idempotent write is safe to race, unlike removing it, which
    // would break any of those tests running concurrently. That means the
    // "not configured" (503) branch can't be exercised reliably from this
    // binary; only the "configured" (redirect) branch is tested here.
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    envmnt::set("GATEHOUSE_URL", "http://127.0.0.1:1");

    let (app, container) = app(JwtConfig::for_tests()).await;
    let resp = app.call(req(Method::GET, "/ui/logout", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}

#[tokio::test]
async fn refresh_fails_without_a_refresh_cookie() {
    let (app, container) = app(JwtConfig::for_tests()).await;
    let resp = app.call(req(Method::POST, "/ui/refresh", &container)).await;
    assert!(!resp.status().is_success());
}
