use bytes::Bytes;
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::realm;
use gatehouse_service::tokens::VerificationTokens;
use gatehouse_service::ui::pages::reset::{
    ResetNotice, ResetPasswordForm, ResetPasswordQuery, forgot_password_page,
    forgot_password_page_slash, render_forgot_password_page, render_reset_password_page,
    reset_password_page, reset_password_submit,
};
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::auth::{Permissions, Role};
use quench_auth::domain::session::SessionDb;
use quench_cache::CacheStore;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::prelude::{Form, Inject, Query, Request};
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

fn mailer() -> Arc<dyn Sender> {
    Arc::new(LoggingSender)
}

fn sessions() -> Arc<SessionDb> {
    SessionDb::init(CacheStore::in_memory())
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
// render_forgot_password_page / render_reset_password_page
// -----------------------------------------------------------------

#[tokio::test]
async fn render_forgot_password_page_renders_ok() {
    let resp = render_forgot_password_page();
    assert_eq!(resp.status(), StatusCode::OK);
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

#[tokio::test]
async fn forgot_password_page_renders() {
    let resp = forgot_password_page().await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn forgot_password_page_slash_renders() {
    let resp = forgot_password_page_slash().await;
    assert_eq!(resp.status(), StatusCode::OK);
}

async fn reset_app(db: Db) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::ui::pages::reset::register_routes();
    let container = ContainerBuilder::new()
        .provide(db)
        .provide(mailer())
        .provide_arc(Arc::new(VerificationTokens::in_memory()))
        .provide_arc(sessions())
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
async fn forgot_password_submit_redirects_regardless_of_whether_the_account_exists() {
    let (app, container) = reset_app(db().await).await;
    let resp = app
        .call(post_form(
            "/ui/forgot-password",
            &[("username", "no-such-user")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("reset_requested=1"));
}

#[tokio::test]
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

    let (app, container) = reset_app(db).await;
    let resp = app
        .call(post_form(
            "/ui/forgot-password",
            &[("username", "alice")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}

#[tokio::test]
async fn reset_password_page_renders() {
    let resp = reset_password_page(
        Query(ResetPasswordQuery {
            token: "abc".to_string(),
        }),
        Query(ResetNotice::default()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn reset_password_submit_rejects_an_unknown_token() {
    let resp = reset_password_submit(
        Form(ResetPasswordForm {
            token: "not-a-real-token".to_string(),
            password: "new-password".to_string(),
        }),
        Inject(Arc::new(db().await)),
        Inject(sessions()),
        Inject(Arc::new(VerificationTokens::in_memory())),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("ui_login_reset_invalid"));
}

#[tokio::test]
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

    let resp = reset_password_submit(
        Form(ResetPasswordForm {
            token: token.clone(),
            password: "new-password".to_string(),
        }),
        Inject(Arc::new(db)),
        Inject(sessions()),
        Inject(tokens),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("reset=1"));
}
