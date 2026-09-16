//! Unit tests for `routers/ui/mod.rs`'s root redirects.
//!
//! `is_ui_authenticated`'s `config.auth_enabled` is a plain field on the
//! `JwtConfig` instance each test constructs, so these mutate that directly
//! rather than the `SERVICE_AUTH_ENABLED` env var - no cross-test race to
//! guard against, since nothing here is process-global.
//!
//! `tests/unit.rs` is a separate test binary from `tests/integration.rs`
//! (no shared `support` module), so this file builds its own minimal
//! `discover_and_mount` + hand-built `Request` helpers rather than reaching
//! across binaries.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use std::sync::Arc;

async fn app(jwt_config: JwtConfig) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    conveyor_service::routers::ui::register_routes();
    let container = ContainerBuilder::new()
        .provide(jwt_config)
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

#[tokio::test]
async fn root_redirects_to_login_when_there_is_no_session_and_auth_is_enabled() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;

    let (app, container) = app(config).await;

    let resp = app.call(req(Method::GET, "/ui", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .into_hyper()
        .headers()
        .get("location")
        .expect("redirect location")
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.contains("login"), "{location}");
}

#[tokio::test]
async fn root_slash_redirects_home_when_auth_is_disabled() {
    let config = JwtConfig::for_tests();
    assert!(!config.auth_enabled, "for_tests should default auth off");

    let (app, container) = app(config).await;

    let resp = app.call(req(Method::GET, "/ui/", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .into_hyper()
        .headers()
        .get("location")
        .expect("redirect location")
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.ends_with("/ui/home"), "{location}");
}
