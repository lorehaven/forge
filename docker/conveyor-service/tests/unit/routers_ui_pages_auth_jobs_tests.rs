//! HTTP-level tests for `routers/ui/pages/auth.rs`'s `auth_status` and
//! `routers/ui/pages/jobs.rs`'s log-viewer fragment. Both handlers are
//! `pub(super)`, reachable only through the mounted `ui::scope()`, not
//! directly. `login`/`login_slash`/`callback` need a registered `SsoConfig`
//! this test app doesn't provide, so they're not exercised here.

use actix_web::http::StatusCode;
use actix_web::web::Data;
use actix_web::{App, test as actix_test};
use conveyor_service::routers::ui;
use quench_auth::prelude::JwtConfig;

fn app_with(
    config: JwtConfig,
) -> App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    App::new()
        .app_data(Data::new(config.clone()))
        .service(ui::scope(config))
}

#[actix_web::test]
async fn auth_status_reports_the_dev_admin_bypass_when_auth_is_disabled() {
    let config = JwtConfig::for_tests();
    assert!(!config.auth_enabled);
    let app = actix_test::init_service(app_with(config)).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/status")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["username"], "dev");
    assert_eq!(body["roles"][0], "admin");
}

#[actix_web::test]
async fn auth_status_is_anonymous_when_auth_is_enabled_and_there_is_no_session() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let app = actix_test::init_service(app_with(config)).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/status")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["authenticated"], false);
    assert!(body["username"].is_null());
    assert!(body["roles"].as_array().unwrap().is_empty());
}

#[actix_web::test]
async fn job_log_viewer_connects_to_the_streams_own_sse_endpoint() {
    let config = JwtConfig::for_tests();
    let app = actix_test::init_service(app_with(config)).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/jobs/job-42/log")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8 html");
    assert!(html.contains(r#"id="log-job-42""#));
    assert!(html.contains("jobs/job-42/stream"));
    assert!(html.contains("sse-connect"));
}

#[actix_web::test]
async fn job_log_viewer_offers_a_raw_view_and_a_copy_button() {
    let config = JwtConfig::for_tests();
    let app = actix_test::init_service(app_with(config)).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/jobs/job-42/log")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8 html");

    // "Open raw" points straight at the API's own raw endpoint, in a new tab.
    assert!(html.contains("jobs/job-42/raw"));
    assert!(html.contains(r#"target="_blank""#));
    assert!(html.contains(r#"rel="noopener""#));

    // Copy reads the log element by the same id it was just asserted to have.
    assert!(html.contains("navigator.clipboard.writeText"));
    assert!(html.contains("log-job-42"));
}

#[actix_web::test]
async fn job_log_viewer_redirects_to_login_when_auth_is_enabled_and_there_is_no_session() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let app = actix_test::init_service(app_with(config)).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/jobs/job-42/log")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}
