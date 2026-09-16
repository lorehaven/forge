//! Light HTTP-level coverage of `routers/ui`'s page handlers via the real
//! discovered router - `pub(super)` visibility means these can only be
//! reached through the mounted routes, not called directly.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::domain::sso_client::SsoConfig;
use quench_http::body::InboundBody;
use quench_http::di::{Container, ContainerBuilder};
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use std::sync::Arc;

async fn app(auth_enabled: bool) -> (Arc<dyn Endpoint>, Arc<Container>) {
    switchboard_service::routers::gpu::register_routes();
    switchboard_service::routers::models::register_routes();
    switchboard_service::routers::vllm::register_routes();
    switchboard_service::routers::ui::register_routes();

    let mut jwt_config = JwtConfig::for_tests();
    jwt_config.auth_enabled = auth_enabled;

    let container = ContainerBuilder::new()
        .provide(jwt_config)
        .provide(SsoConfig::init())
        .build()
        .await
        .unwrap();
    let container = Arc::new(container);

    let app = quench_starter::http::discover_and_mount("/");
    (app, container)
}

fn get(path: &str, container: &Arc<Container>) -> Request {
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[tokio::test]
async fn root_redirects_to_home_when_auth_is_disabled() {
    let (app, container) = app(false).await;
    let resp = app.call(get("/ui", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let hyper_resp = resp.into_hyper();
    let location = hyper_resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.ends_with("/ui/home"));
}

#[tokio::test]
async fn root_slash_behaves_the_same_as_root() {
    let (app, container) = app(false).await;
    let resp = app.call(get("/ui/", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}

#[tokio::test]
async fn root_redirects_to_login_when_auth_is_enabled_and_unauthenticated() {
    let (app, container) = app(true).await;
    let resp = app.call(get("/ui", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let hyper_resp = resp.into_hyper();
    let location = hyper_resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(!location.ends_with("/ui/home"));
}

#[tokio::test]
async fn home_page_renders_ok_when_auth_is_disabled() {
    use http_body_util::BodyExt;

    let (app, container) = app(false).await;
    let resp = app.call(get("/ui/home", &container)).await;
    assert!(resp.status().is_success());

    let collected = resp.into_hyper().into_body().collect().await.unwrap();
    let html = String::from_utf8(collected.to_bytes().to_vec()).unwrap();
    assert!(html.contains("ui_home_title"));
}

#[tokio::test]
async fn models_dashboard_page_renders_ok_when_auth_is_disabled() {
    let (app, container) = app(false).await;
    let resp = app.call(get("/ui/models/dashboard", &container)).await;
    assert!(resp.status().is_success());
}

#[tokio::test]
async fn vllm_manage_page_renders_ok_when_auth_is_disabled() {
    let (app, container) = app(false).await;
    let resp = app.call(get("/ui/vllm/manage", &container)).await;
    assert!(resp.status().is_success());
}

#[tokio::test]
async fn auth_status_reports_the_dev_bypass_identity_when_auth_is_disabled() {
    use http_body_util::BodyExt;

    let (app, container) = app(false).await;
    let resp = app.call(get("/ui/status", &container)).await;
    assert!(resp.status().is_success());

    let collected = resp.into_hyper().into_body().collect().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&collected.to_bytes()).unwrap();
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["username"], "dev");
}

#[tokio::test]
async fn auth_status_reports_unauthenticated_without_a_session_cookie_when_auth_is_enabled() {
    use http_body_util::BodyExt;

    let (app, container) = app(true).await;
    let resp = app.call(get("/ui/status", &container)).await;
    assert!(resp.status().is_success());

    let collected = resp.into_hyper().into_body().collect().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&collected.to_bytes()).unwrap();
    assert_eq!(body["authenticated"], false);
}

#[tokio::test]
async fn logout_route_is_reachable() {
    let (app, container) = app(false).await;
    let resp = app.call(get("/ui/logout", &container)).await;
    // With no GATEHOUSE_URL configured in this test environment,
    // `logout_delegation` correctly reports 503 rather than redirecting -
    // the point here is just that the route is wired up and doesn't 404 or
    // panic into a 500.
    assert_ne!(resp.status(), StatusCode::NOT_FOUND);
    assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
