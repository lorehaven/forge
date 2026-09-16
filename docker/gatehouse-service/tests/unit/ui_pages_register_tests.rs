use bytes::Bytes;
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::tokens::VerificationTokens;
use gatehouse_service::ui::pages::register::{
    Notice, VerifyQuery, known_error_key, register_page, register_page_slash, render_register_page,
    verify,
};
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::prelude::{Inject, Query, Request};
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

fn mailer() -> Arc<dyn Sender> {
    Arc::new(LoggingSender)
}

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    String::from_utf8(collected.to_bytes().to_vec()).expect("utf8")
}

fn location(resp: quench_http::response::Response) -> String {
    resp.into_hyper()
        .headers()
        .get("location")
        .expect("location header")
        .to_str()
        .expect("utf8")
        .to_string()
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

#[tokio::test]
async fn register_page_renders() {
    let resp = register_page(Query(Notice::default())).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn register_page_slash_renders() {
    let resp = register_page_slash(Query(Notice::default())).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

async fn register_app(db: Db) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::ui::pages::register::register_routes();
    let container = ContainerBuilder::new()
        .provide(catalog())
        .provide(db)
        .provide(mailer())
        .provide_arc(Arc::new(VerificationTokens::in_memory()))
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn post_form(
    path: &str,
    pairs: &[(&str, &str)],
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let encoded = serde_urlencoded::to_string(pairs).unwrap();
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::from(encoded)),
        container.clone(),
    )
}

#[tokio::test]
async fn register_submit_rejects_an_invalid_email() {
    let (app, container) = register_app(db().await).await;
    let resp = app
        .call(post_form(
            "/ui/register",
            &[
                ("username", "alice"),
                ("password", "correct-horse"),
                ("email", "not-an-email"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("ui_register_error_email_invalid"));
}

#[tokio::test]
async fn register_submit_creates_the_account_and_redirects_to_login() {
    let (app, container) = register_app(db().await).await;
    let resp = app
        .call(post_form(
            "/ui/register",
            &[
                ("username", "alice"),
                ("password", "correct-horse"),
                ("email", "alice@example.com"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("registered=1"));
}

#[tokio::test]
async fn verify_rejects_an_unknown_token() {
    let resp = verify(
        Query(VerifyQuery {
            token: "not-a-real-token".to_string(),
        }),
        Inject(Arc::new(db().await)),
        Inject(Arc::new(VerificationTokens::in_memory())),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("ui_login_verify_invalid"));
}
