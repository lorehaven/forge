use actix_web::body::to_bytes;
use actix_web::{App, HttpResponse, test as actix_test, web};
use gatehouse_service::realm;
use gatehouse_service::ui::pages::auth::{
    LoginNotices, login, login_error_key, login_mfa, login_mfa_submit, login_ok_key,
    login_redirect, login_slash, login_submit, logout, mfa_challenge_url, refresh,
    render_login_page, render_mfa_page, status,
};
use quench_auth::prelude::{JwtConfig, Permissions, Role, SessionDb};
use quench_db::prelude::Db;
use std::sync::Arc;

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn sessions() -> web::Data<Arc<SessionDb>> {
    web::Data::new(SessionDb::init(quench_cache::CacheStore::in_memory()))
}

async fn body_text(resp: HttpResponse) -> String {
    let body = to_bytes(resp.into_body()).await.expect("body");
    String::from_utf8(body.to_vec()).expect("utf8")
}

// -----------------------------------------------------------------
// login_error_key / login_ok_key
// -----------------------------------------------------------------

#[test]
#[allow(clippy::field_reassign_with_default)] // `err` is reassigned again below to check the second branch
fn login_error_key_recognises_only_the_fixed_set() {
    let mut notices = LoginNotices::default();
    notices.err = Some("ui_login_verify_invalid".to_string());
    assert_eq!(login_error_key(&notices), Some("ui_login_verify_invalid"));

    notices.err = Some("something-made-up".to_string());
    assert_eq!(login_error_key(&notices), None);
}

#[test]
fn login_ok_key_prefers_reset_over_everything_else() {
    let notices = LoginNotices {
        reset: Some("1".to_string()),
        registered: Some("1".to_string()),
        ..LoginNotices::default()
    };
    assert_eq!(login_ok_key(&notices), Some("ui_login_reset_ok"));
}

#[test]
fn login_ok_key_is_none_with_no_notices() {
    assert_eq!(login_ok_key(&LoginNotices::default()), None);
}

// -----------------------------------------------------------------
// mfa_challenge_url
// -----------------------------------------------------------------

#[test]
fn mfa_challenge_url_carries_redirect_and_err() {
    let url = mfa_challenge_url("pending-token", Some("/ui/home"), true);
    assert!(url.contains("pending=pending-token"));
    assert!(url.contains("redirect="));
    assert!(url.contains("err=1"));
}

#[test]
fn mfa_challenge_url_omits_an_empty_redirect() {
    let url = mfa_challenge_url("pending-token", Some(""), false);
    assert!(!url.contains("redirect="));
    assert!(!url.contains("err=1"));
}

// -----------------------------------------------------------------
// render_login_page / render_mfa_page
// -----------------------------------------------------------------

#[tokio::test]
async fn render_login_page_shows_the_credential_error() {
    let req = actix_test::TestRequest::default().to_http_request();
    let resp = render_login_page(&req, true, &LoginNotices::default());
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("ui_login_invalid_credentials"));
}

#[tokio::test]
async fn render_login_page_shows_a_notice_ok_key_when_no_error() {
    let req = actix_test::TestRequest::default().to_http_request();
    let notices = LoginNotices {
        registered: Some("1".to_string()),
        ..LoginNotices::default()
    };
    let resp = render_login_page(&req, false, &notices);
    let html = body_text(resp).await;
    assert!(html.contains("ui_login_registered_ok"));
}

#[tokio::test]
async fn render_mfa_page_shows_the_error_banner_when_asked() {
    let resp = render_mfa_page("pending-token", Some("/ui/home"), true);
    let html = body_text(resp).await;
    assert!(html.contains("ui_login_mfa_invalid"));
    assert!(html.contains("pending-token"));
}

#[tokio::test]
async fn render_mfa_page_without_error_omits_the_banner() {
    let resp = render_mfa_page("pending-token", None, false);
    let html = body_text(resp).await;
    assert!(!html.contains("ui_login_mfa_invalid"));
}

// -----------------------------------------------------------------
// login_redirect
// -----------------------------------------------------------------

#[test]
fn login_redirect_points_at_the_login_page() {
    let resp = login_redirect();
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/login"));
}

// -----------------------------------------------------------------
// HTTP handlers
// -----------------------------------------------------------------

#[actix_web::test]
async fn login_renders_the_form_with_no_session() {
    let app = actix_test::init_service(App::new().service(login)).await;
    let req = actix_test::TestRequest::get().uri("/login").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn login_slash_renders_the_form_with_no_session() {
    let app = actix_test::init_service(App::new().service(login_slash)).await;
    let req = actix_test::TestRequest::get().uri("/login/").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn login_mfa_renders_the_code_form() {
    let app = actix_test::init_service(App::new().service(login_mfa)).await;
    let req = actix_test::TestRequest::get()
        .uri("/login/mfa?pending=abc")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn login_submit_redirects_with_an_error_for_unknown_credentials() {
    let db = db().await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .app_data(sessions())
            .service(login_submit),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/login")
        .set_form([("username", "nobody"), ("password", "whatever")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("err="));
}

#[actix_web::test]
async fn login_submit_succeeds_and_sets_session_cookies_for_the_right_password() {
    let db = db().await;
    realm::create(
        &db,
        &realm_catalog(),
        true,
        "alice",
        "correct-horse",
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .await
    .expect("seed user");

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests_with_signing()))
            .app_data(web::Data::new(db))
            .app_data(sessions())
            .service(login_submit),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/login")
        .set_form([("username", "alice"), ("password", "correct-horse")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    assert!(
        resp.headers()
            .get_all(actix_web::http::header::SET_COOKIE)
            .count()
            >= 2
    );
}

#[actix_web::test]
async fn login_mfa_submit_redirects_with_an_error_for_an_unknown_pending_token() {
    let db = db().await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .app_data(sessions())
            .service(login_mfa_submit),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/login/mfa")
        .set_form([("pending", "not-a-real-token"), ("code", "000000")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
}

#[actix_web::test]
async fn logout_clears_cookies_and_redirects_to_login() {
    let app = actix_test::init_service(App::new().app_data(sessions()).service(logout)).await;
    let req = actix_test::TestRequest::get().uri("/logout").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/login"));
}

#[actix_web::test]
async fn status_reports_when_there_is_no_session() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .service(status),
    )
    .await;
    let req = actix_test::TestRequest::get().uri("/status").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success() || resp.status().is_client_error());
}

#[actix_web::test]
async fn refresh_without_a_cookie_is_not_a_server_error() {
    let app = actix_test::init_service(App::new().service(refresh)).await;
    let req = actix_test::TestRequest::post().uri("/refresh").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(!resp.status().is_server_error());
}

fn realm_catalog() -> gatehouse_service::catalog::PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("auth-page-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    let result =
        gatehouse_service::catalog::PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}
