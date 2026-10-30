use gatehouse_service::PublicBase;
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::test_support::RecordingSender;
use gatehouse_service::tokens::VerificationTokens;
use gatehouse_service::ui::pages::register::{VerifyQuery, verify};
use quench_core::bytes::Bytes;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::prelude::{Inject, Query, Request};
use std::sync::Arc;

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn catalog() -> PermissionCatalog {
    let dir = std::env::temp_dir().join(format!(
        "register-test-{}",
        quench_core::uuid::Uuid::new_v4()
    ));
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
// HTTP handlers
// -----------------------------------------------------------------

async fn register_app(db: Db) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    register_app_with(db, mailer()).await
}

async fn register_app_with(
    db: Db,
    mailer: Arc<dyn Sender>,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::ui::pages::register::register_routes();
    let container = ContainerBuilder::new()
        .provide(catalog())
        .provide(db)
        .provide(mailer)
        .provide(PublicBase::resolve("https://mail.example.test", ""))
        .provide(gatehouse_service::RateLimiter::in_memory())
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
    post_form_with_headers(path, pairs, HeaderMap::new(), container)
}

fn post_form_with_headers(
    path: &str,
    pairs: &[(&str, &str)],
    headers: HeaderMap,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let encoded = quench_serde::urlencoded::to_string(pairs).unwrap();
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(encoded)),
        container.clone(),
    )
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
async fn verification_link_uses_the_configured_base_not_the_request_host() {
    let recorder = Arc::new(RecordingSender::default());
    let (app, container) = register_app_with(db().await, recorder.clone()).await;

    let mut forged = HeaderMap::new();
    forged.insert("host", "evil.example".parse().unwrap());
    forged.insert("x-forwarded-host", "evil.example".parse().unwrap());
    forged.insert("x-forwarded-proto", "http".parse().unwrap());
    let request = post_form_with_headers(
        "/ui/register",
        &[
            ("username", "alice"),
            ("password", "correct-horse"),
            ("email", "alice@example.com"),
        ],
        forged,
        &container,
    );
    let resp = app.call(request).await;
    assert_eq!(resp.status(), StatusCode::FOUND);

    let sent = recorder.sent();
    assert_eq!(sent.len(), 1, "one verification mail");
    assert!(
        sent[0].link.starts_with("https://mail.example.test/"),
        "link must come from configuration, got {}",
        sent[0].link
    );
    assert!(!sent[0].link.contains("evil.example"));
}

fn register_with_cookie(
    email: &str,
    cookie: Option<&str>,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    if let Some(cookie) = cookie {
        headers.insert("cookie", cookie.parse().unwrap());
    }
    post_form_with_headers(
        "/ui/register",
        &[
            ("username", "alice"),
            ("password", "correct-horse"),
            ("email", email),
        ],
        headers,
        container,
    )
}

#[quench_test::test]
async fn the_verification_email_uses_the_browsers_language() {
    let recorder = Arc::new(RecordingSender::default());
    let (app, container) = register_app_with(db().await, recorder.clone()).await;
    let resp = app
        .call(register_with_cookie(
            "alice@example.com",
            Some("qlocale=pl-PL; other=1"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert_eq!(recorder.sent()[0].locale.as_deref(), Some("pl-PL"));
}

#[quench_test::test]
async fn an_unsupported_or_missing_browser_language_means_no_preference() {
    for cookie in [Some("qlocale=xx-XX"), Some("theme=dark"), None] {
        let recorder = Arc::new(RecordingSender::default());
        let (app, container) = register_app_with(db().await, recorder.clone()).await;
        app.call(register_with_cookie(
            "alice@example.com",
            cookie,
            &container,
        ))
        .await;
        assert_eq!(recorder.sent()[0].locale, None, "{cookie:?}");
    }
}

#[quench_test::test]
async fn registration_refuses_addresses_that_could_never_be_delivered_to() {
    for bad in [
        "no-at-sign",
        "a b@example.com",
        "a@@example.com",
        "@example.com",
        "a@",
        "zoë@example.com",
    ] {
        let recorder = Arc::new(RecordingSender::default());
        let (app, container) = register_app_with(db().await, recorder.clone()).await;
        let resp = app.call(register_with_cookie(bad, None, &container)).await;
        assert_eq!(resp.status(), StatusCode::FOUND, "{bad:?}");
        assert!(
            location(resp).contains("ui_register_error_email_invalid"),
            "{bad:?}"
        );
        assert!(recorder.sent().is_empty(), "{bad:?}: no account, no email");
    }
}

#[quench_test::test]
async fn the_address_is_trimmed_before_it_is_stored_and_emailed() {
    let recorder = Arc::new(RecordingSender::default());
    let (app, container) = register_app_with(db().await, recorder.clone()).await;
    app.call(register_with_cookie(
        "  alice@example.com \t",
        None,
        &container,
    ))
    .await;
    assert_eq!(recorder.sent()[0].to, "alice@example.com");
}

#[quench_test::test]
async fn a_failing_mail_server_does_not_fail_the_registration() {
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
    let (app, container) = register_app_with(db().await, Arc::new(Failing)).await;
    let resp = app
        .call(register_with_cookie("alice@example.com", None, &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("registered=1"));
}

fn register_named(
    username: &str,
    email: &str,
    ip: Option<&str>,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    if let Some(ip) = ip {
        headers.insert("x-real-ip", ip.parse().unwrap());
    }
    post_form_with_headers(
        "/ui/register",
        &[
            ("username", username),
            ("password", "correct-horse"),
            ("email", email),
        ],
        headers,
        container,
    )
}

#[quench_test::test]
async fn one_address_cannot_be_flooded_through_many_usernames() {
    let recorder = Arc::new(RecordingSender::default());
    let db = db().await;
    let (app, container) = register_app_with(db.clone(), recorder.clone()).await;
    let limit = gatehouse_service::ratelimit::policy::REGISTER_EMAIL.max;

    for n in 0..limit {
        let resp = app
            .call(register_named(
                &format!("victim{n}"),
                "victim@example.com",
                None,
                &container,
            ))
            .await;
        assert!(location(resp).contains("registered=1"), "registration {n}");
    }
    let resp = app
        .call(register_named(
            "one-too-many",
            "victim@example.com",
            None,
            &container,
        ))
        .await;
    let target = location(resp);
    assert!(
        target.contains("err=ui_register_error_rate_limited"),
        "{target}"
    );
    assert_eq!(recorder.sent().len(), limit, "no email past the limit");
    assert!(
        gatehouse_service::realm::get(&db, "one-too-many")
            .await
            .is_err(),
        "the refused registration must not create an account"
    );

    // A different address is fine.
    let resp = app
        .call(register_named(
            "someone",
            "someone@example.com",
            None,
            &container,
        ))
        .await;
    assert!(location(resp).contains("registered=1"));
}

#[quench_test::test]
async fn one_client_cannot_register_without_end() {
    let recorder = Arc::new(RecordingSender::default());
    let (app, container) = register_app_with(db().await, recorder.clone()).await;
    let limit = gatehouse_service::ratelimit::policy::REGISTER_IP.max;
    let mut refused = None;
    for n in 0..limit + 3 {
        let resp = app
            .call(register_named(
                &format!("u{n}"),
                &format!("u{n}@example.com"),
                Some("192.0.2.77"),
                &container,
            ))
            .await;
        if location(resp).contains("err=ui_register_error_rate_limited") {
            refused = Some(n);
            break;
        }
    }
    assert_eq!(refused, Some(limit));
    let resp = app
        .call(register_named(
            "elsewhere",
            "e@example.com",
            Some("192.0.2.78"),
            &container,
        ))
        .await;
    assert!(location(resp).contains("registered=1"));
}
