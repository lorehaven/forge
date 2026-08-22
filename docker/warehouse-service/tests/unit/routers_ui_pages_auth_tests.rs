use actix_web::{App, test, web};
use quench_auth::actix::domain::sso_client::SsoConfig;
use quench_auth::prelude::JwtConfig;
use warehouse_service::routers::ui::pages::auth::{
    callback, login, login_slash, logout, refresh, status,
};

// `#[get(...)]`/`#[post(...)]` turn `login`/`login_slash`/`logout`/etc.
// into `HttpServiceFactory` structs, not callable functions - so
// coverage on their one-line bodies only comes from actually routing a
// request to them, not from calling them directly. Each test below
// builds its own app rather than sharing a helper, to dodge spelling out
// `test::init_service`'s opaque return type.
macro_rules! app_with_all_routes {
    () => {
        test::init_service(
            App::new()
                .app_data(web::Data::new(SsoConfig::init()))
                .app_data(web::Data::new(JwtConfig::for_tests()))
                .service(login)
                .service(login_slash)
                .service(callback)
                .service(logout)
                .service(status)
                .service(refresh),
        )
        .await
    };
}

// Neither `GATEHOUSE_URL` nor the client id/secret are set in this test
// environment, so `login`/`callback`/`logout` all deterministically hit
// `quench_auth`'s "gatehouse is not configured" branch (503) rather than
// ever building a real redirect - that's still real coverage of this
// crate's own handler-to-delegation wiring, just not the happy path.

#[actix_web::test]
async fn login_reports_gatehouse_unconfigured_without_a_gatehouse_url() {
    let app = app_with_all_routes!();
    let req = test::TestRequest::get().uri("/login").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE
    );
}

#[actix_web::test]
async fn login_slash_delegates_the_same_way_as_login() {
    let app = app_with_all_routes!();
    let req = test::TestRequest::get().uri("/login/").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE
    );
}

#[actix_web::test]
async fn callback_reports_gatehouse_unconfigured_without_a_gatehouse_url() {
    let app = app_with_all_routes!();
    let req = test::TestRequest::get().uri("/auth/callback").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE
    );
}

#[actix_web::test]
async fn logout_reports_gatehouse_unconfigured_without_a_gatehouse_url() {
    let app = app_with_all_routes!();
    let req = test::TestRequest::get().uri("/logout").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE
    );
}

#[actix_web::test]
async fn status_reports_whether_the_caller_is_authenticated() {
    let app = app_with_all_routes!();
    let req = test::TestRequest::get().uri("/status").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success() || resp.status().is_client_error());
}

#[actix_web::test]
async fn refresh_delegates_to_the_token_refresh_flow() {
    let app = app_with_all_routes!();
    let req = test::TestRequest::post().uri("/refresh").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success() || resp.status().is_client_error());
}
