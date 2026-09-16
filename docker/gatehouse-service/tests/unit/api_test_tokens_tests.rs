use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use std::sync::Arc;

fn env_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn app() -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::api::test_tokens::register_routes();
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(JwtConfig::for_tests_with_signing())
            .build()
            .await
            .unwrap(),
    );
    (quench_starter::http::discover_and_mount("/"), container)
}

fn post_json(
    path: &str,
    body: serde_json::Value,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        headers,
        InboundBody::from_bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
        container.clone(),
    )
}

#[tokio::test]
async fn mint_is_not_found_when_test_mode_is_off() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::remove_var("GATEHOUSE_TEST_MODE") };

    let (app, container) = app().await;
    let resp = app
        .call(post_json(
            "/api/v1/test/token",
            serde_json::json!({ "sub": "alice", "scope": "gatehouse:read" }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn mint_issues_a_token_with_the_requested_claims_when_test_mode_is_on() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("GATEHOUSE_TEST_MODE", "true") };

    let (app, container) = app().await;
    let resp = app
        .call(post_json(
            "/api/v1/test/token",
            serde_json::json!({ "sub": "alice", "aud": ["sage"], "scope": "sage:read" }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    let body: serde_json::Value = serde_json::from_slice(&collected.to_bytes()).unwrap();
    assert!(body["access_token"].as_str().is_some_and(|t| !t.is_empty()));

    unsafe { std::env::remove_var("GATEHOUSE_TEST_MODE") };
}

#[tokio::test]
async fn mint_honors_explicit_iat_and_exp_overrides() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("GATEHOUSE_TEST_MODE", "true") };

    let (app, container) = app().await;
    let resp = app
        .call(post_json(
            "/api/v1/test/token",
            serde_json::json!({ "sub": "alice", "scope": "sage:read", "iat": 1, "exp": 2 }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    unsafe { std::env::remove_var("GATEHOUSE_TEST_MODE") };
}
