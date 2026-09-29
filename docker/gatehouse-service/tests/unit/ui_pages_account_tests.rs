use bytes::Bytes;
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::realm::{self, RealmError, begin_mfa_enrollment};
use gatehouse_service::test_support::{RecordingSender, TEST_KEY_MATERIAL, auth_disabled_guard};
use gatehouse_service::tokens::VerificationTokens;
use gatehouse_service::ui::pages::account::{
    Notice, error_page, known_error_key, notice_banner, render_account_page, render_mfa_enroll_page,
};
use gatehouse_service::{PublicBase, RateLimiter};
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
    account_app_with_mailer(config, db, Arc::new(LoggingSender)).await
}

async fn account_app_with_mailer(
    config: JwtConfig,
    db: Db,
    mailer: Arc<dyn Sender>,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::ui::pages::account::register_routes();
    let container = ContainerBuilder::new()
        .provide(config)
        .provide(catalog())
        .provide(db)
        .provide(mailer)
        .provide(PublicBase::resolve("https://mail.example.test", ""))
        .provide(RateLimiter::in_memory())
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

fn post_multipart(
    path: &str,
    fields: &[(&str, &str)],
    file: Option<&[u8]>,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let boundary = "testboundary";
    let mut body: Vec<u8> = Vec::new();
    for (name, value) in fields {
        body.extend(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    if let Some(bytes) = file {
        body.extend(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"avatar_file\"; filename=\"a.png\"\r\nContent-Type: image/png\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend(bytes);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{boundary}--\r\n").as_bytes());

    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        format!("multipart/form-data; boundary={boundary}")
            .parse()
            .unwrap(),
    );
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(body)),
        container.clone(),
    )
}

const PNG_BYTES: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

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
        .call(post_multipart(
            "/ui/account",
            &[
                ("display_name", "Alice A."),
                ("timezone", "Europe/Warsaw"),
                ("preferred_locale", "pl-PL"),
            ],
            None,
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("ok=saved"));
}

#[tokio::test]
async fn save_account_rejects_a_timezone_or_locale_outside_the_lists() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin").await;
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;
    for field in [("timezone", "Mars/Olympus"), ("preferred_locale", "xx-XX")] {
        let resp = app
            .call(post_multipart("/ui/account", &[field], None, &container))
            .await;
        assert!(location(resp).contains("err=ui_account_error_invalid"));
    }
}

#[tokio::test]
async fn save_account_rejects_a_non_image_and_an_oversized_avatar() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin").await;
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;

    let resp = app
        .call(post_multipart(
            "/ui/account",
            &[],
            Some(b"<svg onload=x>"),
            &container,
        ))
        .await;
    assert!(location(resp).contains("err=ui_account_error_avatar_type"));

    let mut big = PNG_BYTES.to_vec();
    big.resize(300 * 1024, 0);
    let resp = app
        .call(post_multipart("/ui/account", &[], Some(&big), &container))
        .await;
    assert!(location(resp).contains("err=ui_account_error_avatar_size"));
}

#[tokio::test]
async fn an_uploaded_avatar_is_served_back_and_absent_ones_404() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin").await;
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;

    let resp = app.call(get("/ui/account/avatar", &container)).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let resp = app
        .call(post_multipart(
            "/ui/account",
            &[],
            Some(PNG_BYTES),
            &container,
        ))
        .await;
    assert!(location(resp).contains("ok=saved"));

    let resp = app.call(get("/ui/account/avatar", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let hyper = resp.into_hyper();
    assert_eq!(hyper.headers()["content-type"], "image/png");
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

#[tokio::test]
async fn change_password_needs_the_current_password() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin").await;
    let (app, container) = account_app(JwtConfig::for_tests(), db.clone()).await;

    // seed_user's password is "password".
    let resp = app
        .call(post_form(
            "/ui/account/password",
            &[
                ("current_password", "wrong"),
                ("new_password", "n3w-secret"),
                ("confirm_password", "n3w-secret"),
            ],
            &container,
        ))
        .await;
    assert!(location(resp).contains("err=ui_account_error_current_password"));

    let resp = app
        .call(post_form(
            "/ui/account/password",
            &[
                ("current_password", "password"),
                ("new_password", "n3w-secret"),
                ("confirm_password", "different"),
            ],
            &container,
        ))
        .await;
    assert!(location(resp).contains("err=ui_account_error_password_mismatch"));

    let resp = app
        .call(post_form(
            "/ui/account/password",
            &[
                ("current_password", "password"),
                ("new_password", "n3w-secret"),
                ("confirm_password", "n3w-secret"),
            ],
            &container,
        ))
        .await;
    assert!(location(resp).contains("ok=password_changed"));
    let user = realm::get(&db, "admin").await.expect("user");
    assert!(user.verify_password("n3w-secret"));
}

// -----------------------------------------------------------------
// Security notices
// -----------------------------------------------------------------

/// A user named `admin` (the bypass identity) with an address; confirmed or not.
async fn seed_with_address(db: &Db, address: Option<&str>, confirmed: bool) {
    realm::create(
        db,
        &catalog(),
        false,
        "admin",
        "password",
        vec![Role::User],
        Permissions::new(),
        address.map(str::to_string),
    )
    .await
    .expect("seed user");
    if confirmed {
        realm::mark_email_verified(db, "admin")
            .await
            .expect("verify");
    }
}

fn recording() -> (Arc<RecordingSender>, Arc<dyn Sender>) {
    let recorder = Arc::new(RecordingSender::default());
    let sender: Arc<dyn Sender> = recorder.clone();
    (recorder, sender)
}

const NEW_PASSWORD: [(&str, &str); 3] = [
    ("current_password", "password"),
    ("new_password", "a-brand-new-password"),
    ("confirm_password", "a-brand-new-password"),
];

#[tokio::test]
async fn changing_your_password_tells_the_confirmed_address() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("alice@example.test"), true).await;
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;

    let resp = app
        .call(post_form("/ui/account/password", &NEW_PASSWORD, &container))
        .await;
    assert!(location(resp).contains("ok=password_changed"));

    let sent = recorder.sent_of("password-changed");
    assert_eq!(sent.len(), 1, "{:?}", recorder.sent());
    assert_eq!(sent[0].to, "alice@example.test");
    assert_eq!(sent[0].username, "admin");
    assert_eq!(sent[0].link, "", "a notice carries no link");
}

#[tokio::test]
async fn no_password_notice_for_an_unconfirmed_or_missing_address() {
    for (address, confirmed) in [(Some("alice@example.test"), false), (None, false)] {
        let _guard = auth_disabled_guard().await;
        let db = db().await;
        seed_with_address(&db, address, confirmed).await;
        let (recorder, sender) = recording();
        let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;
        let resp = app
            .call(post_form("/ui/account/password", &NEW_PASSWORD, &container))
            .await;
        assert!(
            location(resp).contains("ok=password_changed"),
            "{address:?}"
        );
        assert!(
            recorder.sent().is_empty(),
            "{address:?}: {:?}",
            recorder.sent()
        );
    }
}

#[tokio::test]
async fn a_failed_password_change_sends_nothing() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("alice@example.test"), true).await;
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;

    let wrong_current = [
        ("current_password", "not-my-password"),
        ("new_password", "a-brand-new-password"),
        ("confirm_password", "a-brand-new-password"),
    ];
    let resp = app
        .call(post_form(
            "/ui/account/password",
            &wrong_current,
            &container,
        ))
        .await;
    assert!(location(resp).contains("err="));
    let mismatch = [
        ("current_password", "password"),
        ("new_password", "one"),
        ("confirm_password", "two"),
    ];
    let resp = app
        .call(post_form("/ui/account/password", &mismatch, &container))
        .await;
    assert!(location(resp).contains("err="));
    assert!(recorder.sent().is_empty());
}

#[tokio::test]
async fn the_notice_uses_the_saved_language() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("alice@example.test"), true).await;
    realm::update(
        &db,
        &catalog(),
        &sessions(),
        "admin",
        true,
        "admin",
        realm::UserChanges {
            preferred_locale: Some("de-DE".into()),
            ..Default::default()
        },
    )
    .await
    .expect("set locale");
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;
    app.call(post_form("/ui/account/password", &NEW_PASSWORD, &container))
        .await;
    assert_eq!(recorder.sent()[0].locale.as_deref(), Some("de-DE"));
}

#[tokio::test]
async fn turning_mfa_on_and_off_tells_the_confirmed_address() {
    let _guard = auth_disabled_guard().await;
    with_key();
    let db = db().await;
    seed_with_address(&db, Some("alice@example.test"), true).await;
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;

    let (secret, _) = begin_mfa_enrollment("admin").expect("begin enrollment");
    let code = current_totp_code(&secret, "admin");
    let resp = app
        .call(post_form(
            "/ui/account/mfa/enroll",
            &[("secret", secret.as_str()), ("code", code.as_str())],
            &container,
        ))
        .await;
    assert!(location(resp).contains("ok=mfa_enabled"));
    assert_eq!(recorder.sent_of("mfa-enabled").len(), 1);

    let resp = app
        .call(post_form("/ui/account/mfa/disable", &[], &container))
        .await;
    assert!(location(resp).contains("ok=mfa_disabled"));
    assert_eq!(recorder.sent_of("mfa-disabled").len(), 1);
    assert_eq!(recorder.sent_of("mfa-enabled")[0].to, "alice@example.test");
}

#[tokio::test]
async fn a_wrong_enrolment_code_sends_nothing_and_disabling_when_off_stays_quiet() {
    let _guard = auth_disabled_guard().await;
    with_key();
    let db = db().await;
    seed_with_address(&db, Some("alice@example.test"), true).await;
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;

    let (secret, _) = begin_mfa_enrollment("admin").expect("begin enrollment");
    app.call(post_form(
        "/ui/account/mfa/enroll",
        &[("secret", secret.as_str()), ("code", "000000")],
        &container,
    ))
    .await;
    // MFA was never on, so "turned off" would be a false alarm.
    app.call(post_form("/ui/account/mfa/disable", &[], &container))
        .await;
    assert!(recorder.sent().is_empty(), "{:?}", recorder.sent());
}

// -----------------------------------------------------------------
// Changing your email address
// -----------------------------------------------------------------

async fn ask_for_address(
    app: &Arc<dyn Endpoint>,
    container: &Arc<quench_http::di::Container>,
    email: &str,
    password: &str,
) -> String {
    let resp = app
        .call(post_form(
            "/ui/account/email",
            &[("email", email), ("email_current_password", password)],
            container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    location(resp)
}

#[tokio::test]
async fn asking_for_a_new_address_mails_the_new_one_and_changes_nothing_yet() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("old@example.test"), true).await;
    let (recorder, sender) = recording();
    let (app, container) =
        account_app_with_mailer(JwtConfig::for_tests(), db.clone(), sender).await;

    let target = ask_for_address(&app, &container, "new@example.test", "password").await;
    assert!(target.contains("ok=email_change_sent"), "{target}");

    let sent = recorder.sent_of("email-change");
    assert_eq!(sent.len(), 1, "{:?}", recorder.sent());
    assert_eq!(
        sent[0].to, "new@example.test",
        "the link goes to the NEW address"
    );
    assert!(
        sent[0].link.starts_with("https://mail.example.test/")
            && sent[0].link.contains("/confirm-email?token="),
        "{}",
        sent[0].link
    );
    let user = realm::get(&db, "admin").await.unwrap();
    assert_eq!(
        user.email.as_deref(),
        Some("old@example.test"),
        "nothing changed yet"
    );
    assert!(recorder.sent_of("email-changed").is_empty());
}

#[tokio::test]
async fn the_current_password_is_required_and_a_wrong_one_sends_nothing() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("old@example.test"), true).await;
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;
    for password in ["wrong", ""] {
        let target = ask_for_address(&app, &container, "new@example.test", password).await;
        assert!(
            target.contains("err=ui_account_error_current_password"),
            "{password:?}: {target}"
        );
    }
    assert!(recorder.sent().is_empty());
}

#[tokio::test]
async fn an_undeliverable_address_is_refused_before_anything_else() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("old@example.test"), true).await;
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;
    for bad in ["nonsense", "a b@example.test", "zoë@example.test", ""] {
        let target = ask_for_address(&app, &container, bad, "password").await;
        assert!(
            target.contains("err=ui_register_error_email_invalid"),
            "{bad:?}: {target}"
        );
    }
    assert!(recorder.sent().is_empty());
}

#[tokio::test]
async fn your_own_confirmed_address_is_not_a_change() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("Old@Example.test"), true).await;
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;
    let target = ask_for_address(&app, &container, "old@example.test", "password").await;
    assert!(
        target.contains("err=ui_account_error_email_same"),
        "{target}"
    );
    assert!(recorder.sent().is_empty());
}

#[tokio::test]
async fn an_unconfirmed_address_may_be_asked_for_again_to_confirm_it() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("old@example.test"), false).await;
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;
    let target = ask_for_address(&app, &container, "old@example.test", "password").await;
    assert!(target.contains("ok=email_change_sent"), "{target}");
    assert_eq!(recorder.sent_of("email-change").len(), 1);
}

#[tokio::test]
async fn a_person_can_only_ask_a_few_times_an_hour() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("old@example.test"), true).await;
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;
    let limit = gatehouse_service::ratelimit::policy::EMAIL_CHANGE_USER.max;
    for n in 0..limit {
        let target = ask_for_address(
            &app,
            &container,
            &format!("new{n}@example.test"),
            "password",
        )
        .await;
        assert!(
            target.contains("ok=email_change_sent"),
            "request {n}: {target}"
        );
    }
    let target = ask_for_address(&app, &container, "onemore@example.test", "password").await;
    assert!(
        target.contains("err=ui_account_error_rate_limited"),
        "{target}"
    );
    assert_eq!(recorder.sent_of("email-change").len(), limit);
}

#[tokio::test]
async fn a_failing_mail_server_is_reported_to_the_person_asking() {
    struct Failing;
    #[async_trait::async_trait]
    impl Sender for Failing {
        async fn send(
            &self,
            _: &gatehouse_service::email::Recipient<'_>,
            _: &gatehouse_service::email::Mail<'_>,
        ) -> Result<(), gatehouse_service::email::SendError> {
            Err(gatehouse_service::email::SendError::transient("smtp down"))
        }
    }
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("old@example.test"), true).await;
    let (app, container) =
        account_app_with_mailer(JwtConfig::for_tests(), db, Arc::new(Failing)).await;
    let target = ask_for_address(&app, &container, "new@example.test", "password").await;
    assert!(
        target.contains("err=ui_account_error_email_not_sent"),
        "{target}"
    );
}

#[tokio::test]
async fn the_account_page_shows_the_address_its_status_and_the_change_form() {
    let _guard = auth_disabled_guard().await;
    for (address, confirmed, expected) in [
        (Some("me@example.test"), true, "ui_account_email_confirmed"),
        (
            Some("me@example.test"),
            false,
            "ui_account_email_unconfirmed",
        ),
        (None, false, "ui_account_email_none"),
    ] {
        let db = db().await;
        seed_with_address(&db, address, confirmed).await;
        let (app, container) = account_app(JwtConfig::for_tests(), db).await;
        let resp = app.call(get("/ui/account", &container)).await;
        let html = body_text(resp).await;
        assert!(html.contains(expected), "{address:?} {confirmed}: {html}");
        assert!(html.contains("action=\"/ui/account/email\"") || html.contains("/account/email"));
        assert!(html.contains("name=\"email\""));
        assert!(html.contains("name=\"email_current_password\""));
        if let Some(address) = address {
            assert!(html.contains(address));
        }
        // The two password boxes on the page must not share an id.
        assert!(html.contains("id=\"email_current_password\""));
        assert!(html.contains("id=\"current_password\""));
    }
}

#[test]
fn the_email_change_outcomes_are_known_to_the_page() {
    assert!(
        notice_banner(&Notice {
            err: None,
            ok: Some("email_change_sent".into()),
        })
        .is_some()
    );
    for key in [
        "ui_account_error_email_same",
        "ui_account_error_email_not_sent",
        "ui_account_error_rate_limited",
        "ui_register_error_email_invalid",
    ] {
        assert_eq!(known_error_key(key), Some(key), "{key}");
    }
    assert!(known_error_key("ui_account_error_made_up").is_none());
}

// -----------------------------------------------------------------
// Notification preferences
// -----------------------------------------------------------------

use gatehouse_service::notify::{Preferences, catalog as notification_catalog};

fn template(id: &str) -> &'static gatehouse_service::notify::Template {
    notification_catalog::find(id).unwrap()
}

/// The `<input ...>` tag for a checkbox named `name`, to read its `checked` state.
fn checkbox_tag(html: &str, name: &str) -> String {
    html.split("<input")
        .find(|chunk| chunk.contains(&format!("name=\"{name}\"")))
        .unwrap_or_else(|| panic!("no checkbox {name} in the page"))
        .split('>')
        .next()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn the_account_page_lists_every_kind_with_its_default() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("me@example.test"), true).await;
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;
    let html = body_text(app.call(get("/ui/account", &container)).await).await;

    assert!(html.contains("ui_account_notifications_title"));
    assert!(html.contains("/account/notifications"));
    for t in notification_catalog::all() {
        assert!(html.contains(&t.label_key()), "{}", t.id);
        let tag = checkbox_tag(&html, &format!("sub_{}", t.id));
        assert_eq!(tag.contains("checked"), t.default_on, "{}: {tag}", t.id);
    }
    assert!(
        !html.contains("ui_account_notifications_no_address"),
        "there is a confirmed address"
    );
}

#[tokio::test]
async fn the_account_page_reflects_saved_choices() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("me@example.test"), true).await;
    let prefs = Preferences::new(&db);
    prefs
        .set("admin", template("conveyor.run.failed"), false)
        .await
        .unwrap();
    prefs
        .set("admin", template("conveyor.run.succeeded"), true)
        .await
        .unwrap();
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;
    let html = body_text(app.call(get("/ui/account", &container)).await).await;
    assert!(!checkbox_tag(&html, "sub_conveyor.run.failed").contains("checked"));
    assert!(checkbox_tag(&html, "sub_conveyor.run.succeeded").contains("checked"));
}

#[tokio::test]
async fn saving_records_only_departures_from_the_defaults() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("me@example.test"), true).await;
    let (app, container) = account_app(JwtConfig::for_tests(), db.clone()).await;

    // Only "succeeded" ticked: failed is switched off, succeeded switched on.
    let resp = app
        .call(post_form(
            "/ui/account/notifications",
            &[("sub_conveyor.run.succeeded", "on")],
            &container,
        ))
        .await;
    assert!(location(resp).contains("ok=notifications_saved"));
    let overrides = Preferences::new(&db).overrides("admin").await.unwrap();
    assert_eq!(overrides.get("conveyor.run.failed"), Some(&false));
    assert_eq!(overrides.get("conveyor.run.succeeded"), Some(&true));

    // Ticking exactly the defaults again leaves nothing stored.
    let resp = app
        .call(post_form(
            "/ui/account/notifications",
            &[("sub_conveyor.run.failed", "on")],
            &container,
        ))
        .await;
    assert!(location(resp).contains("ok=notifications_saved"));
    assert!(
        Preferences::new(&db)
            .overrides("admin")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn unknown_fields_in_the_form_are_ignored() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("me@example.test"), true).await;
    let (app, container) = account_app(JwtConfig::for_tests(), db.clone()).await;
    let resp = app
        .call(post_form(
            "/ui/account/notifications",
            &[
                ("sub_conveyor.run.failed", "on"),
                ("sub_made.up.kind", "on"),
                ("sub_../../etc/passwd", "on"),
                ("username", "someone-else"),
            ],
            &container,
        ))
        .await;
    assert!(location(resp).contains("ok=notifications_saved"));
    assert!(
        Preferences::new(&db)
            .overrides("admin")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        Preferences::new(&db)
            .overrides("someone-else")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn without_a_confirmed_address_the_page_says_why_nothing_will_arrive() {
    let _guard = auth_disabled_guard().await;
    for (address, confirmed) in [(Some("me@example.test"), false), (None, false)] {
        let db = db().await;
        seed_with_address(&db, address, confirmed).await;
        let (app, container) = account_app(JwtConfig::for_tests(), db).await;
        let html = body_text(app.call(get("/ui/account", &container)).await).await;
        assert!(
            html.contains("ui_account_notifications_no_address"),
            "{address:?}"
        );
    }
}

#[test]
fn the_saved_notice_is_known_to_the_page() {
    assert!(
        notice_banner(&Notice {
            err: None,
            ok: Some("notifications_saved".into()),
        })
        .is_some()
    );
}
