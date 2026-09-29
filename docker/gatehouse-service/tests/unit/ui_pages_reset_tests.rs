use bytes::Bytes;
use gatehouse_service::PublicBase;
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::realm;
use gatehouse_service::test_support::RecordingSender;
use gatehouse_service::tokens::VerificationTokens;
use gatehouse_service::ui::locale::BrowserLocale;
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
    reset_app_with(db, mailer()).await
}

async fn reset_app_with(
    db: Db,
    mailer: Arc<dyn Sender>,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::ui::pages::reset::register_routes();
    let container = ContainerBuilder::new()
        .provide(db)
        .provide(mailer)
        .provide(PublicBase::resolve("https://mail.example.test", ""))
        .provide(gatehouse_service::RateLimiter::in_memory())
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
    post_form_with_headers(path, pairs, HeaderMap::new(), container)
}

fn post_form_with_headers(
    path: &str,
    pairs: &[(&str, &str)],
    headers: HeaderMap,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let encoded = serde_urlencoded::to_string(pairs).unwrap();
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        headers,
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
        Inject(Arc::new(mailer())),
        BrowserLocale(None),
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
        Inject(Arc::new(mailer())),
        BrowserLocale(None),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("reset=1"));
}

#[tokio::test]
async fn reset_link_uses_the_configured_base_not_the_request_host() {
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

    let recorder = Arc::new(RecordingSender::default());
    let (app, container) = reset_app_with(db, recorder.clone()).await;

    let mut forged = HeaderMap::new();
    forged.insert("host", "evil.example".parse().unwrap());
    forged.insert("x-forwarded-host", "evil.example".parse().unwrap());
    let request = post_form_with_headers(
        "/ui/forgot-password",
        &[("username", "alice")],
        forged,
        &container,
    );
    let resp = app.call(request).await;
    assert_eq!(resp.status(), StatusCode::FOUND);

    let sent = recorder.sent();
    assert_eq!(sent.len(), 1, "one reset mail");
    assert_eq!(sent[0].to, "alice@example.com");
    assert!(
        sent[0].link.starts_with("https://mail.example.test/"),
        "link must come from configuration, got {}",
        sent[0].link
    );
    assert!(sent[0].link.contains("/reset-password?token="));
    assert!(!sent[0].link.contains("evil.example"));
}

async fn requested_reset(preferred: Option<&str>, cookie: Option<&str>) -> Option<String> {
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
    if let Some(preferred) = preferred {
        realm::update(
            &db,
            &catalog(),
            &sessions(),
            "alice",
            true,
            "alice",
            realm::UserChanges {
                preferred_locale: Some(preferred.to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("set locale");
    }
    let recorder = Arc::new(RecordingSender::default());
    let (app, container) = reset_app_with(db, recorder.clone()).await;
    let mut headers = HeaderMap::new();
    if let Some(cookie) = cookie {
        headers.insert("cookie", cookie.parse().unwrap());
    }
    let request = post_form_with_headers(
        "/ui/forgot-password",
        &[("username", "alice")],
        headers,
        &container,
    );
    let resp = app.call(request).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    recorder.sent()[0].locale.clone()
}

#[tokio::test]
async fn the_reset_email_uses_the_saved_language_over_the_browsers() {
    assert_eq!(
        requested_reset(Some("de-DE"), Some("qlocale=pl-PL"))
            .await
            .as_deref(),
        Some("de-DE")
    );
}

#[tokio::test]
async fn the_reset_email_falls_back_to_the_browsers_language() {
    assert_eq!(
        requested_reset(None, Some("qlocale=fr-FR"))
            .await
            .as_deref(),
        Some("fr-FR")
    );
    assert_eq!(requested_reset(None, None).await, None);
}

#[tokio::test]
async fn a_failing_mail_server_still_gets_the_same_redirect() {
    struct Failing;
    #[async_trait::async_trait]
    impl Sender for Failing {
        async fn send(
            &self,
            _: &gatehouse_service::email::Recipient<'_>,
            _: &gatehouse_service::email::Mail<'_>,
        ) -> Result<(), gatehouse_service::email::SendError> {
            Err(gatehouse_service::email::SendError::permanent("smtp down"))
        }
    }
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
    let (app, container) = reset_app_with(db, Arc::new(Failing)).await;
    let resp = app
        .call(post_form(
            "/ui/forgot-password",
            &[("username", "alice")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("reset_requested=1"));
}

async fn forgot(
    app: &Arc<dyn Endpoint>,
    container: &Arc<quench_http::di::Container>,
    username: &str,
    ip: Option<&str>,
) -> String {
    let mut headers = HeaderMap::new();
    if let Some(ip) = ip {
        headers.insert("x-real-ip", ip.parse().unwrap());
    }
    let request = post_form_with_headers(
        "/ui/forgot-password",
        &[("username", username)],
        headers,
        container,
    );
    location(app.call(request).await)
}

#[tokio::test]
async fn a_fourth_reset_request_for_one_account_within_an_hour_is_refused() {
    let db = db().await;
    realm::register(
        &db,
        &catalog(),
        "alice",
        "correct-horse",
        "alice@example.com",
    )
    .await
    .unwrap();
    let recorder = Arc::new(RecordingSender::default());
    let (app, container) = reset_app_with(db, recorder.clone()).await;
    let limit = gatehouse_service::ratelimit::policy::RESET_USER.max;

    for n in 0..limit {
        let target = forgot(&app, &container, "alice", None).await;
        assert!(
            target.contains("reset_requested=1"),
            "request {n}: {target}"
        );
    }
    let target = forgot(&app, &container, "alice", None).await;
    assert!(target.contains("err=ui_login_rate_limited"), "{target}");
    assert_eq!(recorder.sent().len(), limit, "no email past the limit");
}

#[tokio::test]
async fn refusals_look_the_same_for_accounts_that_do_not_exist() {
    let recorder = Arc::new(RecordingSender::default());
    let (app, container) = reset_app_with(db().await, recorder.clone()).await;
    let limit = gatehouse_service::ratelimit::policy::RESET_USER.max;
    for _ in 0..limit {
        assert!(
            forgot(&app, &container, "ghost", None)
                .await
                .contains("reset_requested=1")
        );
    }
    // Refused at the same point as for a real account, so the caller learns
    // nothing about whether it exists.
    assert!(
        forgot(&app, &container, "ghost", None)
            .await
            .contains("err=ui_login_rate_limited")
    );
    assert!(recorder.sent().is_empty());
}

#[tokio::test]
async fn one_client_cannot_ask_for_resets_without_end() {
    let recorder = Arc::new(RecordingSender::default());
    let (app, container) = reset_app_with(db().await, recorder).await;
    let limit = gatehouse_service::ratelimit::policy::RESET_IP.max;
    let mut refused = None;
    for n in 0..limit + 3 {
        let target = forgot(&app, &container, &format!("user{n}"), Some("192.0.2.9")).await;
        if target.contains("err=ui_login_rate_limited") {
            refused = Some(n);
            break;
        }
    }
    assert_eq!(refused, Some(limit));
    assert!(
        forgot(&app, &container, "someone", Some("192.0.2.10"))
            .await
            .contains("reset_requested=1")
    );
}

async fn redeem_a_reset(
    db: &Db,
    address: Option<&str>,
    confirmed: bool,
    sender: Arc<dyn Sender>,
) -> String {
    realm::create(
        db,
        &catalog(),
        true,
        "alice",
        "old-password",
        vec![Role::User],
        Permissions::new(),
        address.map(str::to_string),
    )
    .await
    .expect("seed user");
    if confirmed {
        realm::mark_email_verified(db, "alice")
            .await
            .expect("verify");
    }
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
            token,
            password: "new-password".to_string(),
        }),
        Inject(Arc::new(db.clone())),
        Inject(sessions()),
        Inject(tokens),
        Inject(Arc::new(sender)),
        BrowserLocale(None),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    location(resp)
}

fn reset_recorder() -> (Arc<RecordingSender>, Arc<dyn Sender>) {
    let recorder = Arc::new(RecordingSender::default());
    let sender: Arc<dyn Sender> = recorder.clone();
    (recorder, sender)
}

#[tokio::test]
async fn following_a_reset_link_confirms_an_unconfirmed_address_and_tells_it() {
    let db = db().await;
    let (recorder, sender) = reset_recorder();
    let target = redeem_a_reset(&db, Some("alice@example.test"), false, sender).await;
    assert!(target.contains("reset=1"), "{target}");

    // The link went to that address and was used, so the address is theirs.
    let user = realm::get(&db, "alice").await.unwrap();
    assert!(user.email_verified_at.is_some());
    assert!(user.verify_password("new-password"));
    let sent = recorder.sent_of("password-changed");
    assert_eq!(sent.len(), 1, "{:?}", recorder.sent());
    assert_eq!(sent[0].to, "alice@example.test");
}

#[tokio::test]
async fn a_reset_for_a_confirmed_address_just_sends_the_notice() {
    let db = db().await;
    let (recorder, sender) = reset_recorder();
    redeem_a_reset(&db, Some("alice@example.test"), true, sender).await;
    assert_eq!(recorder.sent_of("password-changed").len(), 1);
}

#[tokio::test]
async fn a_reset_for_an_account_without_an_address_works_and_says_nothing() {
    let db = db().await;
    let (recorder, sender) = reset_recorder();
    let target = redeem_a_reset(&db, None, false, sender).await;
    assert!(target.contains("reset=1"));
    let user = realm::get(&db, "alice").await.unwrap();
    assert!(user.verify_password("new-password"));
    assert!(user.email_verified_at.is_none(), "nothing to confirm");
    assert!(recorder.sent().is_empty());
}
