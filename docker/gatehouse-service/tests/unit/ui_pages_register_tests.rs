use actix_web::body::to_bytes;
use actix_web::{App, HttpResponse, test as actix_test, web};
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::tokens::VerificationTokens;
use gatehouse_service::ui::pages::register::{
    Notice, known_error_key, register_page, register_page_slash, register_submit,
    render_register_page, verify,
};
use quench_db::prelude::Db;
use std::sync::Arc;

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn catalog() -> PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("register-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    let result = PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn mailer() -> web::Data<Arc<dyn Sender>> {
    web::Data::new(Arc::new(LoggingSender) as Arc<dyn Sender>)
}

fn verification_tokens() -> web::Data<Arc<VerificationTokens>> {
    web::Data::new(Arc::new(VerificationTokens::in_memory()))
}

async fn body_text(resp: HttpResponse) -> String {
    let body = to_bytes(resp.into_body()).await.expect("body");
    String::from_utf8(body.to_vec()).expect("utf8")
}

// -----------------------------------------------------------------
// known_error_key / render_register_page
// -----------------------------------------------------------------

#[test]
fn known_error_key_accepts_the_email_validation_key_and_realm_errors() {
    assert_eq!(
        known_error_key("ui_register_error_email_invalid"),
        Some("ui_register_error_email_invalid")
    );
    assert!(known_error_key("made-up-key").is_none());
}

#[tokio::test]
async fn render_register_page_shows_a_known_error() {
    let notice = Notice {
        err: Some("ui_register_error_email_invalid".to_string()),
    };
    let resp = render_register_page(&notice);
    let html = body_text(resp).await;
    assert!(html.contains("ui_register_error_email_invalid"));
}

#[tokio::test]
async fn render_register_page_without_a_notice_has_no_error() {
    let resp = render_register_page(&Notice::default());
    let html = body_text(resp).await;
    assert!(!html.contains("class=\"error\""));
}

// -----------------------------------------------------------------
// HTTP handlers
// -----------------------------------------------------------------

#[actix_web::test]
async fn register_page_renders() {
    let app = actix_test::init_service(App::new().service(register_page)).await;
    let req = actix_test::TestRequest::get().uri("/register").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn register_page_slash_renders() {
    let app = actix_test::init_service(App::new().service(register_page_slash)).await;
    let req = actix_test::TestRequest::get()
        .uri("/register/")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn register_submit_rejects_an_invalid_email() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(catalog()))
            .app_data(web::Data::new(db().await))
            .app_data(mailer())
            .app_data(verification_tokens())
            .service(register_submit),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/register")
        .set_form([
            ("username", "alice"),
            ("password", "correct-horse"),
            ("email", "not-an-email"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("ui_register_error_email_invalid"));
}

#[actix_web::test]
async fn register_submit_creates_the_account_and_redirects_to_login() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(catalog()))
            .app_data(web::Data::new(db().await))
            .app_data(mailer())
            .app_data(verification_tokens())
            .service(register_submit),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/register")
        .set_form([
            ("username", "alice"),
            ("password", "correct-horse"),
            ("email", "alice@example.com"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("registered=1"));
}

#[actix_web::test]
async fn verify_rejects_an_unknown_token() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(db().await))
            .app_data(verification_tokens())
            .service(verify),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/verify?token=not-a-real-token")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("ui_login_verify_invalid"));
}
