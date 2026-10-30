use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::realm::{self, begin_mfa_enrollment};
use gatehouse_service::test_support::{RecordingSender, TEST_KEY_MATERIAL, auth_disabled_guard};
use gatehouse_service::tokens::VerificationTokens;
use gatehouse_service::{PublicBase, RateLimiter};
use quench_auth::domain::auth::{Permissions, Role, User};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_cache::CacheStore;
use quench_core::bytes::Bytes;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::request::Request;
use std::sync::Arc;

fn with_key() {
    quench_core::env::set("GATEHOUSE_KEY_ENCRYPTION_KEY", TEST_KEY_MATERIAL);
}

/// `mfa::totp` is private to that module, so this rebuilds the same TOTP
/// object here to get a code `enable_mfa` will accept - same parameters
/// `mfa.rs` uses (SHA1, 6 digits, 30s step, "Forge" issuer).
fn current_totp_code(secret: &str) -> String {
    quench_crypto::totp::Totp::from_base32(secret)
        .expect("valid base32 secret")
        .code_now()
}

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn catalog() -> PermissionCatalog {
    let dir = std::env::temp_dir().join(format!(
        "account-test-{}",
        quench_core::uuid::Uuid::new_v4()
    ));
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
    let collected = resp
        .into_inner()
        .body
        .collect_limited(usize::MAX)
        .await
        .expect("body");
    String::from_utf8(collected.to_vec()).expect("utf8")
}

fn location(resp: quench_http::response::Response) -> String {
    resp.into_inner()
        .headers
        .get("location")
        .expect("location header")
        .to_str()
        .expect("utf8")
        .to_string()
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
    gatehouse_service::api::pages::register_routes();
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
    let encoded = quench_serde::urlencoded::to_string(pairs).unwrap();
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::from(encoded)),
        container.clone(),
    )
}

/// The enrolment is the one write the client makes itself, through the page API.
fn enroll(secret: &str, code: &str, container: &Arc<quench_http::di::Container>) -> Request {
    let body = quench_serde::json::to_vec(&quench_serde::json!({ "secret": secret, "code": code }));
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    Request::new(
        Method::POST,
        "/api/v1/ui/mfa-enroll".parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(body)),
        container.clone(),
    )
}

async fn account_json(
    app: &Arc<dyn Endpoint>,
    container: &Arc<quench_http::di::Container>,
) -> gatehouse_core::Account {
    let resp = app.call(get("/api/v1/ui/account", container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    quench_serde::json::from_str(&body_text(resp).await).unwrap()
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

#[quench_test::test]
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

#[quench_test::test]
async fn account_page_renders_the_bypass_user_s_profile() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin").await;
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;
    let resp = app.call(get("/ui/account", &container)).await;
    assert!(resp.status() == StatusCode::OK || resp.status() == StatusCode::NOT_FOUND);
    let account = account_json(&app, &container).await;
    assert_eq!(account.username, "admin");
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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
    let hyper = resp.into_inner();
    assert_eq!(hyper.headers.get("content-type").unwrap(), "image/png");
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
async fn turning_mfa_on_and_off_tells_the_confirmed_address() {
    let _guard = auth_disabled_guard().await;
    with_key();
    let db = db().await;
    seed_with_address(&db, Some("alice@example.test"), true).await;
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;

    let (secret, _) = begin_mfa_enrollment("admin").expect("begin enrollment");
    let code = current_totp_code(&secret);
    let resp = app.call(enroll(&secret, &code, &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(recorder.sent_of("mfa-enabled").len(), 1);

    let resp = app
        .call(post_form("/ui/account/mfa/disable", &[], &container))
        .await;
    assert!(location(resp).contains("ok=mfa_disabled"));
    assert_eq!(recorder.sent_of("mfa-disabled").len(), 1);
    assert_eq!(recorder.sent_of("mfa-enabled")[0].to, "alice@example.test");
}

#[quench_test::test]
async fn a_wrong_enrolment_code_sends_nothing_and_disabling_when_off_stays_quiet() {
    let _guard = auth_disabled_guard().await;
    with_key();
    let db = db().await;
    seed_with_address(&db, Some("alice@example.test"), true).await;
    let (recorder, sender) = recording();
    let (app, container) = account_app_with_mailer(JwtConfig::for_tests(), db, sender).await;

    let (secret, _) = begin_mfa_enrollment("admin").expect("begin enrollment");
    let resp = app.call(enroll(&secret, "000000", &container)).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
async fn a_failing_mail_server_is_reported_to_the_person_asking() {
    struct Failing;
    #[quench_core::async_trait]
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

#[quench_test::test]
async fn the_account_has_the_address_and_its_status() {
    let _guard = auth_disabled_guard().await;
    for (address, confirmed) in [
        (Some("me@example.test"), true),
        (Some("me@example.test"), false),
        (None, false),
    ] {
        let db = db().await;
        seed_with_address(&db, address, confirmed).await;
        let (app, container) = account_app(JwtConfig::for_tests(), db).await;
        let account = account_json(&app, &container).await;
        assert_eq!(account.email.as_deref(), address);
        assert_eq!(account.email_verified, confirmed, "{address:?}");
    }
}

// -----------------------------------------------------------------
// Notification preferences
// -----------------------------------------------------------------

use gatehouse_service::notify::{Preferences, catalog as notification_catalog};

fn template(id: &str) -> &'static gatehouse_service::notify::Template {
    notification_catalog::find(id).unwrap()
}

#[quench_test::test]
async fn the_account_lists_every_kind_with_its_default() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_with_address(&db, Some("me@example.test"), true).await;
    let (app, container) = account_app(JwtConfig::for_tests(), db).await;
    let account = account_json(&app, &container).await;

    assert_eq!(
        account.subscriptions.len(),
        notification_catalog::all().len()
    );
    for t in notification_catalog::all() {
        let sub = account
            .subscriptions
            .iter()
            .find(|s| s.id == t.id)
            .unwrap_or_else(|| panic!("{} is missing", t.id));
        assert_eq!(sub.wanted, t.default_on, "{}", t.id);
        assert_eq!(sub.label_key, t.label_key());
        assert_eq!(sub.service, t.service);
    }
    assert!(account.has_notice_address, "there is a confirmed address");
}

#[quench_test::test]
async fn the_account_reflects_saved_choices() {
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
    let account = account_json(&app, &container).await;
    let wanted = |id: &str| {
        account
            .subscriptions
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .wanted
    };
    assert!(!wanted("conveyor.run.failed"));
    assert!(wanted("conveyor.run.succeeded"));
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
async fn without_a_confirmed_address_the_account_says_why_nothing_will_arrive() {
    let _guard = auth_disabled_guard().await;
    for (address, confirmed) in [(Some("me@example.test"), false), (None, false)] {
        let db = db().await;
        seed_with_address(&db, address, confirmed).await;
        let (app, container) = account_app(JwtConfig::for_tests(), db).await;
        let account = account_json(&app, &container).await;
        assert!(!account.has_notice_address, "{address:?}");
    }
}
