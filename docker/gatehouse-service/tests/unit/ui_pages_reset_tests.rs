use actix_web::body::to_bytes;
use actix_web::{App, HttpResponse, test as actix_test, web};
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::realm;
use gatehouse_service::tokens::VerificationTokens;
use gatehouse_service::ui::pages::reset::{
    ResetNotice, forgot_password_page, forgot_password_page_slash, forgot_password_submit,
    render_forgot_password_page, render_reset_password_page, reset_password_page,
    reset_password_submit,
};
use quench_auth::prelude::{Permissions, Role, SessionDb};
use quench_db::prelude::Db;
use std::sync::Arc;

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn catalog() -> gatehouse_service::catalog::PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("reset-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    let result =
        gatehouse_service::catalog::PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn mailer() -> web::Data<Arc<dyn Sender>> {
    web::Data::new(Arc::new(LoggingSender) as Arc<dyn Sender>)
}

fn verification_tokens() -> web::Data<Arc<VerificationTokens>> {
    web::Data::new(Arc::new(VerificationTokens::in_memory()))
}

fn sessions() -> web::Data<Arc<SessionDb>> {
    web::Data::new(SessionDb::init(quench_cache::CacheStore::in_memory()))
}

async fn body_text(resp: HttpResponse) -> String {
    let body = to_bytes(resp.into_body()).await.expect("body");
    String::from_utf8(body.to_vec()).expect("utf8")
}

// -----------------------------------------------------------------
// render_forgot_password_page / render_reset_password_page
// -----------------------------------------------------------------

#[tokio::test]
async fn render_forgot_password_page_renders_ok() {
    let resp = render_forgot_password_page();
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("ui_forgot_password_title"));
}

#[tokio::test]
async fn render_reset_password_page_carries_the_token_and_shows_the_error() {
    let notice = ResetNotice {
        err: Some("ui_reset_error_password_empty".to_string()),
    };
    let resp = render_reset_password_page("a-token", &notice);
    let html = body_text(resp).await;
    assert!(html.contains("a-token"));
    assert!(html.contains("ui_reset_error_password_empty"));
}

#[tokio::test]
async fn render_reset_password_page_without_error_omits_the_banner() {
    let resp = render_reset_password_page("a-token", &ResetNotice::default());
    let html = body_text(resp).await;
    assert!(!html.contains("ui_reset_error_password_empty"));
}

// -----------------------------------------------------------------
// HTTP handlers
// -----------------------------------------------------------------

#[actix_web::test]
async fn forgot_password_page_renders() {
    let app = actix_test::init_service(App::new().service(forgot_password_page)).await;
    let req = actix_test::TestRequest::get()
        .uri("/forgot-password")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn forgot_password_page_slash_renders() {
    let app = actix_test::init_service(App::new().service(forgot_password_page_slash)).await;
    let req = actix_test::TestRequest::get()
        .uri("/forgot-password/")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn forgot_password_submit_redirects_regardless_of_whether_the_account_exists() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(db().await))
            .app_data(mailer())
            .app_data(verification_tokens())
            .service(forgot_password_submit),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/forgot-password")
        .set_form([("username", "no-such-user")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("reset_requested=1"));
}

#[actix_web::test]
async fn forgot_password_submit_sends_a_link_for_a_real_account_with_an_email() {
    let db = db().await;
    realm::register(
        &db,
        &catalog(),
        "alice",
        "correct-horse",
        "alice@example.com",
    )
    .await
    .expect("register");

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(db))
            .app_data(mailer())
            .app_data(verification_tokens())
            .service(forgot_password_submit),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/forgot-password")
        .set_form([("username", "alice")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
}

#[actix_web::test]
async fn reset_password_page_renders() {
    let app = actix_test::init_service(App::new().service(reset_password_page)).await;
    let req = actix_test::TestRequest::get()
        .uri("/reset-password?token=abc")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn reset_password_submit_rejects_an_unknown_token() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(db().await))
            .app_data(sessions())
            .app_data(verification_tokens())
            .service(reset_password_submit),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/reset-password")
        .set_form([("token", "not-a-real-token"), ("password", "new-password")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("ui_login_reset_invalid"));
}

#[actix_web::test]
async fn reset_password_submit_changes_the_password_for_a_valid_token() {
    let db = db().await;
    realm::create(
        &db,
        &catalog(),
        true,
        "alice",
        "old-password",
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .await
    .expect("seed user");

    let tokens = Arc::new(VerificationTokens::in_memory());
    let token = tokens
        .issue(
            gatehouse_service::tokens::PURPOSE_RESET_PASSWORD,
            "alice",
            3600,
        )
        .await
        .expect("issue token");

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(db))
            .app_data(sessions())
            .app_data(web::Data::new(tokens))
            .service(reset_password_submit),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/reset-password")
        .set_form([("token", token.as_str()), ("password", "new-password")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("reset=1"));
}
