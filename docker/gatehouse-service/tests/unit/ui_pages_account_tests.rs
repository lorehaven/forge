use actix_web::body::to_bytes;
use actix_web::{App, HttpResponse, test as actix_test, web};
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::realm::{self, RealmError, begin_mfa_enrollment};
use gatehouse_service::test_support::TEST_KEY_MATERIAL;
use gatehouse_service::ui::pages::account::{
    Notice, account_page, error_page, known_error_key, mfa_disable, mfa_enroll_page,
    mfa_enroll_submit, notice_banner, render_account_page, render_mfa_enroll_page, save_account,
};
use quench_auth::prelude::{JwtConfig, Permissions, Role, SessionDb, User};
use quench_db::prelude::Db;
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

fn sessions() -> web::Data<Arc<SessionDb>> {
    web::Data::new(SessionDb::init(quench_cache::CacheStore::in_memory()))
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

async fn body_text(resp: HttpResponse) -> String {
    let body = to_bytes(resp.into_body()).await.expect("body");
    String::from_utf8(body.to_vec()).expect("utf8")
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
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

// -----------------------------------------------------------------
// HTTP handlers - not signed in
// -----------------------------------------------------------------

#[actix_web::test]
async fn account_page_redirects_to_login_when_not_signed_in() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .app_data(web::Data::new(db().await))
            .service(account_page),
    )
    .await;
    let req = actix_test::TestRequest::get().uri("/account").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

// -----------------------------------------------------------------
// HTTP handlers - auth disabled (bypass claims)
// -----------------------------------------------------------------

#[actix_web::test]
async fn account_page_renders_the_bypass_user_s_profile() {
    let db = db().await;
    seed_user(&db, "admin").await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .service(account_page),
    )
    .await;
    let req = actix_test::TestRequest::get().uri("/account").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn save_account_updates_the_profile_and_redirects() {
    let db = db().await;
    seed_user(&db, "admin").await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(catalog()))
            .app_data(web::Data::new(db))
            .app_data(sessions())
            .service(save_account),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/account")
        .set_form([("display_name", "Alice A.")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("ok=saved"));
}

#[actix_web::test]
async fn mfa_enroll_page_renders_a_fresh_secret() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .service(mfa_enroll_page),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/account/mfa/enroll")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("ui_account_mfa_enroll_title"));
}

#[actix_web::test]
async fn mfa_enroll_submit_rejects_a_wrong_code() {
    with_key();
    let db = db().await;
    seed_user(&db, "admin").await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .service(mfa_enroll_submit),
    )
    .await;
    let (secret, _) = begin_mfa_enrollment("admin").expect("begin enrollment");
    let req = actix_test::TestRequest::post()
        .uri("/account/mfa/enroll")
        .set_form([("secret", secret.as_str()), ("code", "000000")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    // A wrong code re-renders the enroll page with the error banner rather
    // than redirecting.
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("ui_admin_error_mfa_code_invalid"));
}

#[actix_web::test]
async fn mfa_enroll_submit_enables_mfa_with_the_right_code() {
    with_key();
    let db = db().await;
    seed_user(&db, "admin").await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .service(mfa_enroll_submit),
    )
    .await;
    let (secret, _) = begin_mfa_enrollment("admin").expect("begin enrollment");
    let code = current_totp_code(&secret, "admin");
    let req = actix_test::TestRequest::post()
        .uri("/account/mfa/enroll")
        .set_form([("secret", secret.as_str()), ("code", code.as_str())])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("mfa_enabled"));
}

#[actix_web::test]
async fn mfa_disable_turns_mfa_back_off() {
    let db = db().await;
    seed_user(&db, "admin").await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .service(mfa_disable),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/account/mfa/disable")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("mfa_disabled"));
}
