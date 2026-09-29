use bytes::Bytes;
use gatehouse_service::email::Sender;
use gatehouse_service::ratelimit::policy;
use gatehouse_service::realm;
use gatehouse_service::test_support::RecordingSender;
use gatehouse_service::tokens::{PURPOSE_VERIFY_EMAIL, VerificationTokens};
use gatehouse_service::ui::pages::resend::render_resend_page;
use gatehouse_service::{PublicBase, RateLimiter};
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::auth::{Permissions, Role};
use quench_auth::domain::session::SessionDb;
use quench_cache::CacheStore;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::prelude::Request;
use std::sync::Arc;

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn catalog() -> gatehouse_service::catalog::PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("resend-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    let result =
        gatehouse_service::catalog::PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

struct Harness {
    app: Arc<dyn Endpoint>,
    container: Arc<quench_http::di::Container>,
    recorder: Arc<RecordingSender>,
    tokens: Arc<VerificationTokens>,
    db: Db,
}

async fn harness() -> Harness {
    harness_with(Arc::new(RecordingSender::default())).await
}

async fn harness_with(recorder: Arc<RecordingSender>) -> Harness {
    gatehouse_service::ui::pages::resend::register_routes();
    let db = db().await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let mailer: Arc<dyn Sender> = recorder.clone();
    let container = ContainerBuilder::new()
        .provide(db.clone())
        .provide(mailer)
        .provide(PublicBase::resolve("https://mail.example.test", ""))
        .provide(RateLimiter::in_memory())
        .provide_arc(tokens.clone())
        .build()
        .await
        .unwrap();
    Harness {
        app: quench_starter::http::discover_and_mount("/"),
        container: Arc::new(container),
        recorder,
        tokens,
        db,
    }
}

impl Harness {
    async fn register(&self, username: &str, email: &str) {
        realm::register(&self.db, &catalog(), username, "correct-horse", email)
            .await
            .expect("register");
    }

    async fn resend(&self, username: &str, headers: &[(&str, &str)]) -> (StatusCode, String) {
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.insert(
                http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        let encoded = serde_urlencoded::to_string([("username", username)]).unwrap();
        let request = Request::new(
            Method::POST,
            "/ui/resend-verification".parse::<Uri>().unwrap(),
            map,
            quench_http::body::InboundBody::from_bytes(Bytes::from(encoded)),
            self.container.clone(),
        );
        let resp = self.app.call(request).await;
        let status = resp.status();
        let location = resp
            .into_hyper()
            .headers()
            .get("location")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        (status, location)
    }
}

fn ok_location() -> &'static str {
    "resend_requested=1"
}

#[tokio::test]
async fn an_unverified_account_gets_a_fresh_working_link() {
    let h = harness().await;
    h.register("alice", "alice@example.com").await;

    let (status, location) = h.resend("alice", &[]).await;
    assert_eq!(status, StatusCode::FOUND);
    assert!(location.contains(ok_location()), "{location}");

    let sent = h.recorder.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].kind, "verification");
    assert_eq!(sent[0].to, "alice@example.com");
    assert!(
        sent[0].link.starts_with("https://mail.example.test/"),
        "{}",
        sent[0].link
    );
    assert!(sent[0].link.contains("/verify?token="));

    // The link carries a real, single-use verification token for that user.
    let token = sent[0].link.split("token=").nth(1).unwrap();
    let redeemed = h.tokens.redeem(PURPOSE_VERIFY_EMAIL, token).await.unwrap();
    assert_eq!(redeemed.as_deref(), Some("alice"));
}

#[tokio::test]
async fn the_answer_never_depends_on_the_account() {
    // Existing-unverified, verified, unknown, no-address: the same redirect.
    let h = harness().await;
    h.register("unverified", "u@example.com").await;
    h.register("verified", "v@example.com").await;
    realm::mark_email_verified(&h.db, "verified").await.unwrap();
    realm::create(
        &h.db,
        &catalog(),
        true,
        "noaddress",
        "pw",
        vec![],
        Permissions::new(),
        None,
    )
    .await
    .unwrap();

    let mut answers = Vec::new();
    for name in ["unverified", "verified", "nobody-here", "noaddress"] {
        answers.push(h.resend(name, &[("x-real-ip", "203.0.113.1")]).await);
    }
    assert!(answers.iter().all(|a| a == &answers[0]), "{answers:?}");
    assert!(answers[0].1.contains(ok_location()));
    // Only the account that needed it got mail.
    let to: Vec<String> = h.recorder.sent().iter().map(|m| m.to.clone()).collect();
    assert_eq!(to, ["u@example.com"]);
}

#[tokio::test]
async fn admin_and_disabled_accounts_are_not_mailed() {
    let h = harness().await;
    realm::create(
        &h.db,
        &catalog(),
        true,
        "root",
        "pw",
        vec![Role::Admin],
        Permissions::new(),
        Some("root@example.com".into()),
    )
    .await
    .unwrap();
    h.register("frozen", "f@example.com").await;
    realm::set_disabled(&h.db, "frozen", true).await.unwrap();

    for name in ["root", "frozen"] {
        let (status, location) = h.resend(name, &[]).await;
        assert_eq!(status, StatusCode::FOUND);
        assert!(location.contains(ok_location()), "{name}: {location}");
    }
    assert!(h.recorder.sent().is_empty());
}

#[tokio::test]
async fn a_second_request_within_a_minute_is_refused() {
    let h = harness().await;
    h.register("alice", "alice@example.com").await;
    let first = h.resend("alice", &[]).await;
    let second = h.resend("alice", &[]).await;
    assert!(first.1.contains(ok_location()));
    assert!(
        second.1.contains("err=ui_login_rate_limited"),
        "{}",
        second.1
    );
    assert_eq!(h.recorder.sent().len(), 1, "no second email");
}

#[tokio::test]
async fn refusal_is_the_same_for_accounts_that_do_not_exist() {
    let h = harness().await;
    let first = h.resend("ghost", &[]).await;
    let second = h.resend("ghost", &[]).await;
    assert!(first.1.contains(ok_location()));
    assert!(
        second.1.contains("err=ui_login_rate_limited"),
        "{}",
        second.1
    );
}

#[tokio::test]
async fn one_address_is_limited_no_matter_how_many_usernames() {
    let h = harness().await;
    let ip = [("x-real-ip", "198.51.100.20")];
    let mut limited_at = None;
    for n in 0..(policy::RESEND_IP.max + 3) {
        let (_, location) = h.resend(&format!("user{n}"), &ip).await;
        if location.contains("err=ui_login_rate_limited") {
            limited_at = Some(n);
            break;
        }
    }
    assert_eq!(
        limited_at,
        Some(policy::RESEND_IP.max),
        "refused after the limit"
    );

    // Another client is unaffected.
    let (_, location) = h.resend("someone", &[("x-real-ip", "198.51.100.21")]).await;
    assert!(location.contains(ok_location()), "{location}");
}

#[tokio::test]
async fn the_email_follows_the_saved_language_then_the_browsers() {
    let h = harness().await;
    h.register("alice", "alice@example.com").await;
    h.resend("alice", &[("cookie", "qlocale=de-DE")]).await;
    assert_eq!(h.recorder.sent()[0].locale.as_deref(), Some("de-DE"));

    let h = harness().await;
    h.register("bob", "bob@example.com").await;
    realm::update(
        &h.db,
        &catalog(),
        &SessionDb::init(CacheStore::in_memory()),
        "bob",
        true,
        "bob",
        realm::UserChanges {
            preferred_locale: Some("fr-FR".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    h.resend("bob", &[("cookie", "qlocale=de-DE")]).await;
    assert_eq!(h.recorder.sent()[0].locale.as_deref(), Some("fr-FR"));
}

#[tokio::test]
async fn a_failing_mail_server_gets_the_same_redirect() {
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
    gatehouse_service::ui::pages::resend::register_routes();
    let db = db().await;
    realm::register(&db, &catalog(), "alice", "pw", "alice@example.com")
        .await
        .unwrap();
    let mailer: Arc<dyn Sender> = Arc::new(Failing);
    let container = ContainerBuilder::new()
        .provide(db)
        .provide(mailer)
        .provide(PublicBase::resolve("https://mail.example.test", ""))
        .provide(RateLimiter::in_memory())
        .provide_arc(Arc::new(VerificationTokens::in_memory()))
        .build()
        .await
        .unwrap();
    let encoded = serde_urlencoded::to_string([("username", "alice")]).unwrap();
    let resp = quench_starter::http::discover_and_mount("/")
        .call(Request::new(
            Method::POST,
            "/ui/resend-verification".parse::<Uri>().unwrap(),
            HeaderMap::new(),
            quench_http::body::InboundBody::from_bytes(Bytes::from(encoded)),
            Arc::new(container),
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}

#[tokio::test]
async fn the_page_renders_a_form_that_posts_a_username() {
    let resp = render_resend_page();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("resend-verification"), "{html}");
    assert!(html.contains("name=\"username\""));
    assert!(html.contains("ui_resend_submit"));
}
