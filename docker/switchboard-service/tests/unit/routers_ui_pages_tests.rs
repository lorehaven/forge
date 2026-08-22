//! Light HTTP-level coverage of `routers/ui`'s page handlers via the real
//! `ui::scope()` - `pub(super)` visibility means these can only be reached
//! through the mounted routes, not called directly.

use crate::env_support::env_lock;
use actix_web::App;
use actix_web::test as actix_test;
use actix_web::web::Data;
use quench_auth::actix::domain::sso_client::SsoConfig;
use quench_auth::prelude::JwtConfig;
use switchboard_service::routers::ui;

/// Builds the app fresh per test (an `actix_web::test::init_service` result
/// can't be named as a return type without pinning its exact service impl),
/// so this is a macro rather than a function.
macro_rules! app_with_auth_disabled {
    () => {
        actix_test::init_service(
            App::new()
                .app_data(Data::new(JwtConfig::for_tests()))
                .app_data(Data::new(SsoConfig::init()))
                .service(ui::scope()),
        )
        .await
    };
}

#[actix_web::test]
async fn root_redirects_to_home_when_auth_is_disabled() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = app_with_auth_disabled!();
    let req = actix_test::TestRequest::get().uri("/ui").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp.headers().get("Location").unwrap().to_str().unwrap();
    assert!(location.ends_with("/ui/home"));

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn root_slash_behaves_the_same_as_root() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = app_with_auth_disabled!();
    let req = actix_test::TestRequest::get().uri("/ui/").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn root_redirects_to_login_when_auth_is_enabled_and_unauthenticated() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };

    let app = app_with_auth_disabled!();
    let req = actix_test::TestRequest::get().uri("/ui").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp.headers().get("Location").unwrap().to_str().unwrap();
    assert!(!location.ends_with("/ui/home"));

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn home_page_renders_ok_when_auth_is_disabled() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = app_with_auth_disabled!();
    let req = actix_test::TestRequest::get().uri("/ui/home").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("ui_home_title"));

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn models_dashboard_page_renders_ok_when_auth_is_disabled() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = app_with_auth_disabled!();
    let req = actix_test::TestRequest::get()
        .uri("/ui/models/dashboard")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn vllm_manage_page_renders_ok_when_auth_is_disabled() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = app_with_auth_disabled!();
    let req = actix_test::TestRequest::get()
        .uri("/ui/vllm/manage")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn auth_status_reports_the_dev_bypass_identity_when_auth_is_disabled() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = app_with_auth_disabled!();
    let req = actix_test::TestRequest::get()
        .uri("/ui/status")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["username"], "dev");

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn auth_status_reports_unauthenticated_without_a_session_cookie_when_auth_is_enabled() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };

    let app = app_with_auth_disabled!();
    let req = actix_test::TestRequest::get()
        .uri("/ui/status")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["authenticated"], false);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn logout_route_is_reachable() {
    let app = app_with_auth_disabled!();
    let req = actix_test::TestRequest::get()
        .uri("/ui/logout")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    // With no GATEHOUSE_URL configured in this test environment,
    // `logout_delegation` correctly reports 503 rather than redirecting -
    // the point here is just that the route is wired up and doesn't 404 or
    // panic into a 500.
    assert_ne!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
    assert_ne!(
        resp.status(),
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
    );
}
