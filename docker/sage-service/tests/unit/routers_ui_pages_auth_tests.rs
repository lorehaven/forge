use crate::env_support::env_lock;
use actix_web::http::StatusCode;
use actix_web::{App, test, web};
use quench_auth::actix::domain::sso_client::SsoConfig;
use quench_auth::prelude::JwtConfig;
use sage_service::routers::ui::pages::auth::{
    auth_status, callback, login, login_slash, logout, refresh,
};

fn app_data() -> web::Data<SsoConfig> {
    // `SsoConfig::init()` reads `GATEHOUSE_CLIENT_ID`/`_SECRET` from the
    // environment; unset (the default in this test binary), it comes back
    // "unconfigured", which is exactly the branch these tests want to
    // exercise without a real gatehouse to talk to.
    web::Data::new(SsoConfig::init())
}

#[actix_web::test]
async fn auth_status_reports_a_dev_identity_when_auth_is_disabled() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .service(auth_status),
    )
    .await;

    let req = test::TestRequest::get().uri("/status").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["username"], "dev");
}

#[actix_web::test]
async fn auth_status_is_unauthenticated_without_a_session_cookie_when_auth_is_required() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(auth_status),
    )
    .await;

    let req = test::TestRequest::get().uri("/status").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["authenticated"], false);
    assert!(body["username"].is_null());
}

#[actix_web::test]
async fn auth_status_is_unauthenticated_for_a_cookie_that_fails_to_decode() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(auth_status),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/status")
        .cookie(actix_web::cookie::Cookie::new(
            quench_auth::prelude::realm::session_cookie_name(),
            "not-a-real-token",
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["authenticated"], false);
}

#[actix_web::test]
async fn login_delegates_when_sso_is_unconfigured() {
    let app = test::init_service(App::new().app_data(app_data()).service(login)).await;
    let req = test::TestRequest::get().uri("/login").to_request();
    let resp = test::call_service(&app, req).await;
    // Unconfigured SSO can't build a real authorize redirect; any
    // non-panicking response proves `login_delegation` was reached.
    assert!(resp.status().as_u16() >= 300);
}

#[actix_web::test]
async fn login_slash_delegates_the_same_way_as_login() {
    let app = test::init_service(App::new().app_data(app_data()).service(login_slash)).await;
    let req = test::TestRequest::get().uri("/login/").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().as_u16() >= 300);
}

#[actix_web::test]
async fn callback_reports_an_error_without_a_state_cookie() {
    let app = test::init_service(App::new().app_data(app_data()).service(callback)).await;
    let req = test::TestRequest::get()
        .uri("/auth/callback?code=abc&state=xyz")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(!resp.status().is_success());
}

#[actix_web::test]
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

    let app = test::init_service(App::new().service(logout)).await;
    let req = test::TestRequest::get().uri("/logout").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}

#[actix_web::test]
async fn refresh_fails_without_a_refresh_cookie() {
    let app = test::init_service(App::new().service(refresh)).await;
    let req = test::TestRequest::post().uri("/refresh").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(!resp.status().is_success());
}
