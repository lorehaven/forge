use bytes::Bytes;
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::realm::{self, RealmError, begin_mfa_enrollment};
use gatehouse_service::test_support::{TEST_KEY_MATERIAL, auth_disabled_guard};
use gatehouse_service::ui::pages::account::{
    Notice, error_page, known_error_key, notice_banner, render_account_page, render_mfa_enroll_page,
};
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::auth::{Permissions, Role, User};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_cache::CacheStore;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use std::sync::Arc;

fn with_key() {
    envmnt::set("GATEHOUSE_KEY_ENCRYPTION_KEY", TEST_KEY_MATERIAL);
}

/// `mfa::totp` is private to that module, so this rebuilds the same TOTP
/// object here to get a code `enable_mfa` will accept - same parameters
/// `mfa.rs` uses (SHA1, 6 digits, 30s step, "Forge" issuer).
fn current_totp_code(secret: &str, username: &str) -> String {
    use totp_rs::Secret;
    let parsed = Secret::try_from_base32(secret).expect("valid base32 secret");
    totp_rs::Builder::new()
        .with_algorithm(totp_rs::Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(parsed)
        .with_account_name(username.to_string())
        .with_issuer(Some("Forge"))
        .build()
        .expect("build totp")
        .generate_current()
        .to_string()
}

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn catalog() -> PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("account-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    let result = PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn sessions() -> Arc<SessionDb> {
    SessionDb::init(CacheStore::in_memory())
}

async fn seed_user(db: &Db, username: &str) -> User {
    realm::create(
        db,
        &catalog(),
        false,
        username,
        "password",
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .await
    .expect("seed user")
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
// notice_banner / known_error_key
// -----------------------------------------------------------------

#[test]
fn known_error_key_recognises_the_fixed_set() {
    assert_eq!(
        known_error_key(RealmError::MfaCodeInvalid.i18n_key()),
        Some(RealmError::MfaCodeInvalid.i18n_key())
    );
    assert!(known_error_key("made-up").is_none());
}

#[test]
fn notice_banner_shows_every_known_ok_outcome() {
    for ok in ["saved", "mfa_enabled", "mfa_disabled"] {
        let notice = Notice {
            err: None,
            ok: Some(ok.to_string()),
        };
        assert!(notice_banner(&notice).is_some(), "ok={ok}");
    }
    assert!(notice_banner(&Notice::default()).is_none());
}

// -----------------------------------------------------------------
// render_account_page / render_mfa_enroll_page / error_page
// -----------------------------------------------------------------

#[tokio::test]
async fn render_account_page_shows_the_enroll_link_when_mfa_is_off() {
    let user = User::new(
        "alice".to_string(),
        "pw".to_string(),
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .unwrap();
    let resp = render_account_page(&user, &Notice::default());
    let html = body_text(resp).await;
    assert!(html.contains("ui_account_mfa_enable"));
    assert!(!html.contains("ui_account_mfa_disable\""));
}

#[tokio::test]
async fn render_mfa_enroll_page_shows_the_error_when_asked() {
    let resp = render_mfa_enroll_page("SECRET123", "otpauth://totp/x", true);
    let html = body_text(resp).await;
    assert!(html.contains("ui_admin_error_mfa_code_invalid"));
    assert!(html.contains("SECRET123"));
}

#[tokio::test]
async fn render_mfa_enroll_page_without_error_omits_the_banner() {
    let resp = render_mfa_enroll_page("SECRET123", "otpauth://totp/x", false);
    let html = body_text(resp).await;
    assert!(!html.contains("ui_admin_error_mfa_code_invalid"));
}

#[tokio::test]
async fn error_page_renders_with_the_error_s_own_status() {
    let resp = error_page(&RealmError::NotFound);
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// -----------------------------------------------------------------
// HTTP handlers - every route here takes the private `Actor` extractor
// (`ui::pages::account`), so these go through the real discovered router.
// -----------------------------------------------------------------

async fn account_app(
    config: JwtConfig,
    db: Db,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::ui::pages::account::register_routes();
    let container = ContainerBuilder::new()
        .provide(config)
        .provide(catalog())
        .provide(db)
        .provide_arc(sessions())
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

// -----------------------------------------------------------------
// HTTP handlers - not signed in
// -----------------------------------------------------------------

#[tokio::test]
async fn account_page_redirects_to_login_when_not_signed_in() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let (app, container) = account_app(config, db().await).await;
    let resp = app.call(get("/ui/account", &container)).await;
    assert!(resp.status().is_redirection());
}

// -----------------------------------------------------------------
// HTTP handlers - auth disabled (bypass claims)
// -----------------------------------------------------------------

#[tokio::test]
async fn account_page_renders_the_bypass_user_s_profile() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin").await;
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;
    let resp = app.call(get("/ui/account", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn save_account_updates_the_profile_and_redirects() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin").await;
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;
    let resp = app
        .call(post_form(
            "/ui/account",
            &[("display_name", "Alice A.")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("ok=saved"));
}

#[tokio::test]
async fn mfa_enroll_page_renders_a_fresh_secret() {
    let _guard = auth_disabled_guard().await;
    let (app, container) = account_app(JwtConfig::for_tests(), db().await).await;
    let resp = app.call(get("/ui/account/mfa/enroll", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("ui_account_mfa_enroll_title"));
}

#[tokio::test]
async fn mfa_enroll_submit_rejects_a_wrong_code() {
    let _guard = auth_disabled_guard().await;
    with_key();
    let db = db().await;
    seed_user(&db, "admin").await;
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;
    let (secret, _) = begin_mfa_enrollment("admin").expect("begin enrollment");
    let resp = app
        .call(post_form(
            "/ui/account/mfa/enroll",
            &[("secret", secret.as_str()), ("code", "000000")],
            &container,
        ))
        .await;
    // A wrong code re-renders the enroll page with the error banner rather
    // than redirecting.
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("ui_admin_error_mfa_code_invalid"));
}

#[tokio::test]
async fn mfa_enroll_submit_enables_mfa_with_the_right_code() {
    let _guard = auth_disabled_guard().await;
    with_key();
    let db = db().await;
    seed_user(&db, "admin").await;
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;
    let (secret, _) = begin_mfa_enrollment("admin").expect("begin enrollment");
    let code = current_totp_code(&secret, "admin");
    let resp = app
        .call(post_form(
            "/ui/account/mfa/enroll",
            &[("secret", secret.as_str()), ("code", code.as_str())],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("mfa_enabled"));
}

#[tokio::test]
async fn mfa_disable_turns_mfa_back_off() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin").await;
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;
    let resp = app
        .call(post_form("/ui/account/mfa/disable", &[], &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("mfa_disabled"));
}
