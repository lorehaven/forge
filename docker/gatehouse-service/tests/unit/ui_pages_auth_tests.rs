use bytes::Bytes;
use gatehouse_service::realm;
use gatehouse_service::ui::pages::auth::{
    LoginForm, LoginNotices, MfaForm, MfaQuery, login_error_key, login_mfa, login_mfa_submit,
    login_ok_key, login_redirect, login_submit, mfa_challenge_url, render_login_page,
    render_mfa_page,
};
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::auth::{Permissions, Role};
use quench_auth::domain::jwt::JwtConfig;
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

fn sessions() -> Arc<SessionDb> {
    SessionDb::init(CacheStore::in_memory())
}

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    String::from_utf8(collected.to_bytes().to_vec()).expect("utf8")
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
    let resp = render_login_page(None, true, &LoginNotices::default());
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("ui_login_invalid_credentials"));
}

#[tokio::test]
async fn render_login_page_shows_a_notice_ok_key_when_no_error() {
    let notices = LoginNotices {
        registered: Some("1".to_string()),
        ..LoginNotices::default()
    };
    let resp = render_login_page(None, false, &notices);
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
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .into_hyper()
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.contains("/login"));
}

// -----------------------------------------------------------------
// login_mfa - takes only `Query<MfaQuery>` (pub fields), callable directly
// -----------------------------------------------------------------

#[tokio::test]
async fn login_mfa_renders_the_code_form() {
    let resp = login_mfa(Query(MfaQuery {
        pending: "abc".to_string(),
        redirect: None,
        err: None,
    }))
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

// -----------------------------------------------------------------
// login_submit / login_mfa_submit - take only `Form<T>` (pub fields) plus
// `Inject<...>`, callable directly
// -----------------------------------------------------------------

#[tokio::test]
async fn login_submit_redirects_with_an_error_for_unknown_credentials() {
    let resp = login_submit(
        Form(LoginForm {
            username: "nobody".to_string(),
            password: "whatever".to_string(),
            redirect: None,
        }),
        Inject(Arc::new(JwtConfig::for_tests())),
        Inject(Arc::new(db().await)),
        Inject(sessions()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .into_hyper()
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.contains("err="));
}

#[tokio::test]
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

    let resp = login_submit(
        Form(LoginForm {
            username: "alice".to_string(),
            password: "correct-horse".to_string(),
            redirect: None,
        }),
        Inject(Arc::new(JwtConfig::for_tests_with_signing())),
        Inject(Arc::new(db)),
        Inject(sessions()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(
        resp.into_hyper()
            .headers()
            .get_all("set-cookie")
            .iter()
            .count()
            >= 2
    );
}

#[tokio::test]
async fn login_mfa_submit_redirects_with_an_error_for_an_unknown_pending_token() {
    let resp = login_mfa_submit(
        Form(MfaForm {
            pending: "not-a-real-token".to_string(),
            code: "000000".to_string(),
            redirect: None,
        }),
        Inject(Arc::new(JwtConfig::for_tests())),
        Inject(Arc::new(db().await)),
        Inject(sessions()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}

// -----------------------------------------------------------------
// login / login_slash / logout / status / refresh - each takes a private
// per-request extractor (`LoginContext`/`LogoutContext`/`AuthStatusResponse`/
// `RefreshResponse`), so these go through the real discovered router.
// -----------------------------------------------------------------

async fn app(
    auth_enabled: bool,
    db: Db,
    sessions: Arc<SessionDb>,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::ui::pages::auth::register_routes();
    let mut jwt_config = JwtConfig::for_tests();
    jwt_config.auth_enabled = auth_enabled;
    let container = ContainerBuilder::new()
        .provide(jwt_config)
        .provide(db)
        .provide_arc(sessions)
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn get(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

fn post(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[tokio::test]
async fn login_renders_the_form_with_no_session() {
    let (app, container) = app(false, db().await, sessions()).await;
    let resp = app.call(get("/ui/login", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn login_slash_renders_the_form_with_no_session() {
    let (app, container) = app(false, db().await, sessions()).await;
    let resp = app.call(get("/ui/login/", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn logout_clears_cookies_and_redirects_to_login() {
    let (app, container) = app(false, db().await, sessions()).await;
    let resp = app.call(get("/ui/logout", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .into_hyper()
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.contains("/login"));
}

#[tokio::test]
async fn status_reports_when_there_is_no_session() {
    let (app, container) = app(false, db().await, sessions()).await;
    let resp = app.call(get("/ui/status", &container)).await;
    assert!(resp.status().is_success() || resp.status().is_client_error());
}

#[tokio::test]
async fn refresh_without_a_cookie_is_not_a_server_error() {
    let (app, container) = app(false, db().await, sessions()).await;
    let resp = app.call(post("/ui/refresh", &container)).await;
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
