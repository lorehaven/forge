use bytes::Bytes;
use gatehouse_service::test_support::service_auth_env_lock;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::body::InboundBody;
use quench_http::di::{Container, ContainerBuilder};
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use std::sync::Arc;

async fn app(auth_enabled: bool) -> (Arc<dyn Endpoint>, Arc<Container>) {
    gatehouse_service::ui::register_routes();

    let mut jwt_config = JwtConfig::for_tests();
    jwt_config.auth_enabled = auth_enabled;

    let container = ContainerBuilder::new()
        .provide(jwt_config)
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

fn location(resp: quench_http::response::Response) -> String {
    resp.into_hyper()
        .headers()
        .get("location")
        .expect("location header")
        .to_str()
        .expect("utf8")
        .to_string()
}

#[tokio::test]
async fn ui_root_goes_straight_home_when_auth_is_disabled() {
    // `JwtConfig::for_tests()` reads `SERVICE_AUTH_ENABLED` (default
    // "false"), matching how every other service's dev/test bypass
    // works: with auth off, `is_ui_authenticated` treats every request
    // as authenticated.
    let _guard = service_auth_env_lock().lock().await;

    let (app, container) = app(false).await;
    let resp = app.call(get("/ui", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).ends_with("/ui/home"));
}

#[tokio::test]
async fn ui_root_redirects_to_login_when_auth_is_enabled_and_there_is_no_session() {
    let _guard = service_auth_env_lock().lock().await;

    let (app, container) = app(true).await;
    let resp = app.call(get("/ui/", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).ends_with("/ui/login"));
}
