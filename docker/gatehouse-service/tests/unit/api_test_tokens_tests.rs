use actix_web::{App, test, web::Data};
use gatehouse_service::api::test_tokens::mint;
use quench_auth::prelude::JwtConfig;

fn env_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[actix_web::test]
async fn mint_is_not_found_when_test_mode_is_off() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::remove_var("GATEHOUSE_TEST_MODE") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests_with_signing()))
            .service(mint),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/test/token")
        .set_json(serde_json::json!({ "sub": "alice", "scope": "gatehouse:read" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn mint_issues_a_token_with_the_requested_claims_when_test_mode_is_on() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("GATEHOUSE_TEST_MODE", "true") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests_with_signing()))
            .service(mint),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/test/token")
        .set_json(serde_json::json!({
            "sub": "alice",
            "aud": ["sage"],
            "scope": "sage:read"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["access_token"].as_str().is_some_and(|t| !t.is_empty()));

    unsafe { std::env::remove_var("GATEHOUSE_TEST_MODE") };
}

#[actix_web::test]
async fn mint_honors_explicit_iat_and_exp_overrides() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("GATEHOUSE_TEST_MODE", "true") };

    let app = test::init_service(
        App::new()
            .app_data(Data::new(JwtConfig::for_tests_with_signing()))
            .service(mint),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/test/token")
        .set_json(serde_json::json!({
            "sub": "alice",
            "scope": "sage:read",
            "iat": 1,
            "exp": 2
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

    unsafe { std::env::remove_var("GATEHOUSE_TEST_MODE") };
}
