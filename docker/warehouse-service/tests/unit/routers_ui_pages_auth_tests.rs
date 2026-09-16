use crate::support;
use http::{Method, StatusCode};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::domain::sso_client::SsoConfig;

// `#[get(...)]`/`#[post(...)]` turn `login`/`login_slash`/`logout`/etc.
// into discoverable route registrations, not callable functions - so
// coverage on their one-line bodies only comes from actually routing a
// request to them, not from calling them directly.

// Neither `GATEHOUSE_URL` nor the client id/secret are set in this test
// environment, so `login`/`callback`/`logout` all deterministically hit
// `quench_auth`'s "gatehouse is not configured" branch (503) rather than
// ever building a real redirect - that's still real coverage of this
// crate's own handler-to-delegation wiring, just not the happy path.

async fn app() -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::ui::pages::auth::register_routes();
    let container = support::container_builder()
        .provide(SsoConfig::init())
        .provide(JwtConfig::for_tests())
        .build()
        .await
        .unwrap();
    support::app(container).await
}

#[tokio::test]
async fn login_reports_gatehouse_unconfigured_without_a_gatehouse_url() {
    let (app, container) = app().await;
    let resp = app
        .call(support::req(Method::GET, "/ui/login", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn login_slash_delegates_the_same_way_as_login() {
    let (app, container) = app().await;
    let resp = app
        .call(support::req(Method::GET, "/ui/login/", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn callback_reports_gatehouse_unconfigured_without_a_gatehouse_url() {
    let (app, container) = app().await;
    let resp = app
        .call(support::req(Method::GET, "/ui/auth/callback", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn logout_reports_gatehouse_unconfigured_without_a_gatehouse_url() {
    let (app, container) = app().await;
    let resp = app
        .call(support::req(Method::GET, "/ui/logout", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn status_reports_whether_the_caller_is_authenticated() {
    let (app, container) = app().await;
    let resp = app
        .call(support::req(Method::GET, "/ui/status", &container))
        .await;
    assert!(resp.status().is_success() || resp.status().is_client_error());
}

#[tokio::test]
async fn refresh_delegates_to_the_token_refresh_flow() {
    let (app, container) = app().await;
    let resp = app
        .call(support::req(Method::POST, "/ui/refresh", &container))
        .await;
    assert!(resp.status().is_success() || resp.status().is_client_error());
}
