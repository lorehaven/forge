use actix_web::App;
use actix_web::test::{TestRequest, call_service, init_service};
use actix_web::web;
use gatehouse_service::test_support::service_auth_env_lock;
use gatehouse_service::ui::scope;
use quench_auth::prelude::JwtConfig;

fn location(resp: &actix_web::dev::ServiceResponse) -> String {
    resp.headers()
        .get("Location")
        .expect("location header")
        .to_str()
        .expect("utf8")
        .to_string()
}

#[actix_web::test]
async fn ui_root_goes_straight_home_when_auth_is_disabled() {
    // `JwtConfig::for_tests()` reads `SERVICE_AUTH_ENABLED` (default
    // "false"), matching how every other service's dev/test bypass
    // works: with auth off, `is_ui_authenticated` treats every request
    // as authenticated.
    let _guard = service_auth_env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let app = init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .service(scope()),
    )
    .await;

    let req = TestRequest::get().uri("/ui").to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    assert!(location(&resp).ends_with("/ui/home"));

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[actix_web::test]
async fn ui_root_redirects_to_login_when_auth_is_enabled_and_there_is_no_session() {
    let _guard = service_auth_env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };

    let app = init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .service(scope()),
    )
    .await;

    let req = TestRequest::get().uri("/ui/").to_request();
    let resp = call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    assert!(location(&resp).ends_with("/ui/login"));

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}
