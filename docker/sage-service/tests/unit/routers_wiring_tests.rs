//! Unit tests for the pure route-wiring in `routers/mod.rs` and
//! `routers/ui/mod.rs`. `discover_and_mount` plus `register_routes()` walks every
//! route actually linked into this binary, so building the app once
//! exercises every line without needing to send any request; the redirect
//! tests below then check the two root handlers' auth branches directly.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use sage_service::routers;
use std::sync::Arc;

#[tokio::test]
async fn every_router_links_and_mounts_without_panicking() {
    routers::register_routes();
    let container = ContainerBuilder::new()
        .provide(JwtConfig::for_tests())
        .build()
        .await
        .unwrap();
    let _app = quench_starter::http::discover_and_mount("/");
    let _ = Arc::new(container);
}

// ---------------------------------------------------------------------------
// `ui::mod`'s root redirect handlers - reachable at `/ui`/`/ui/`, need only
// `JwtConfig` in the container (unlike most other pages in this module,
// which also touch `Db`/`SwitchboardClient`/etc.).
// ---------------------------------------------------------------------------

async fn app(jwt_config: JwtConfig) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    routers::ui::register_routes();
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

fn req(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[tokio::test]
async fn root_redirects_to_login_when_unauthenticated_and_auth_required() {
    let mut jwt_config = JwtConfig::for_tests();
    jwt_config.auth_enabled = true;
    let (app, container) = app(jwt_config).await;

    for path in ["/ui", "/ui/"] {
        let resp = app.call(req(path, &container)).await;
        assert!(resp.status().is_redirection());
        let location = resp
            .into_hyper()
            .headers()
            .get("location")
            .expect("redirect has a Location header")
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            location.contains("login"),
            "expected a login redirect, got {location}"
        );
    }
}

#[tokio::test]
async fn root_redirects_to_home_when_auth_is_disabled() {
    // `JwtConfig::for_tests()` defaults `auth_enabled` to false, so
    // `is_ui_authenticated` treats every request as authenticated.
    let (app, container) = app(JwtConfig::for_tests()).await;

    let resp = app.call(req("/ui", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .into_hyper()
        .headers()
        .get("location")
        .expect("redirect has a Location header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        location.contains("/ui/home"),
        "expected a home redirect, got {location}"
    );
}
