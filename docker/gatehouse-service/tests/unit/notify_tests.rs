use bytes::Bytes;
use gatehouse_service::email::{
    Mail, Recipient, SendError, Sender, SmtpConfig, SmtpSender, TlsMode,
};
use gatehouse_service::notify::catalog::{self, MAX_URL_LEN, MAX_VALUE_LEN, TEMPLATES};
use gatehouse_service::notify::prefs::Preferences;
use gatehouse_service::notify::unsubscribe::{UNSUBSCRIBE_TTL_SECS, Unsubscribe, issue_link};
use gatehouse_service::notify::{
    Context, Invalid, Outcome, Request as NotifyRequest, Skip, dispatch,
};
use gatehouse_service::ratelimit::policy;
use gatehouse_service::realm;
use gatehouse_service::test_support::{RecordingSender, service_auth_env_lock};
use gatehouse_service::tokens::{PURPOSE_UNSUBSCRIBE, VerificationTokens};
use gatehouse_service::{PublicBase, RateLimiter};
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::auth::{Permissions, Role};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::session::SessionDb;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

const ORIGIN: &str = "https://mail.example.test";

fn base() -> PublicBase {
    PublicBase::resolve(ORIGIN, "")
}

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn catalog_for_realm() -> gatehouse_service::catalog::PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("notify-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    let result =
        gatehouse_service::catalog::PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn vars() -> BTreeMap<String, String> {
    [
        ("project", "forge"),
        ("run", "1234"),
        ("ref", "refs/heads/master"),
        ("url", "https://mail.example.test/conveyor/runs/1234"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

fn failed() -> &'static catalog::Template {
    catalog::find("conveyor.run.failed").unwrap()
}

fn succeeded() -> &'static catalog::Template {
    catalog::find("conveyor.run.succeeded").unwrap()
}

// -- the catalog ------------------------------------------------------

#[test]
fn every_template_is_well_formed() {
    let mut ids: Vec<&str> = TEMPLATES.iter().map(|t| t.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), TEMPLATES.len(), "ids are unique");
    for template in &TEMPLATES {
        assert!(
            template.id.starts_with(&format!("{}.", template.service)),
            "{} belongs under {}",
            template.id,
            template.service
        );
        assert_eq!(
            template.label_key(),
            format!("ui_notification_{}", template.id.replace('.', "_"))
        );
        assert!(!template.vars.is_empty());
    }
    assert!(catalog::find("nope").is_none());
    assert_eq!(catalog::all().len(), TEMPLATES.len());
}

#[test]
fn failures_default_on_and_successes_default_off() {
    assert!(failed().default_on);
    assert!(!succeeded().default_on);
}

#[test]
fn good_variables_validate() {
    assert_eq!(failed().validate(&vars(), ORIGIN), Ok(()));
}

#[test]
fn a_missing_extra_or_empty_variable_is_refused() {
    let mut missing = vars();
    missing.remove("run");
    assert!(
        failed()
            .validate(&missing, ORIGIN)
            .unwrap_err()
            .contains("run")
    );

    let mut extra = vars();
    extra.insert("secret".into(), "x".into());
    assert!(
        failed()
            .validate(&extra, ORIGIN)
            .unwrap_err()
            .contains("secret")
    );

    for blank in ["", "   "] {
        let mut v = vars();
        v.insert("project".into(), blank.into());
        assert!(
            failed().validate(&v, ORIGIN).unwrap_err().contains("empty"),
            "{blank:?}"
        );
    }
    assert!(failed().validate(&BTreeMap::new(), ORIGIN).is_err());
}

#[test]
fn line_breaks_and_control_characters_are_refused_everywhere() {
    for bad in [
        "a\nb",
        "a\rb",
        "tab\there",
        "nul\0",
        "esc\u{1b}[31m",
        "bell\u{7}",
    ] {
        for field in ["project", "run", "ref"] {
            let mut v = vars();
            v.insert(field.into(), bad.into());
            let err = failed().validate(&v, ORIGIN).unwrap_err();
            assert!(err.contains(field), "{field} {bad:?}: {err}");
        }
    }
}

#[test]
fn oversized_values_are_refused() {
    let mut v = vars();
    v.insert("project".into(), "x".repeat(MAX_VALUE_LEN));
    assert!(
        failed().validate(&v, ORIGIN).is_ok(),
        "the limit itself is fine"
    );
    v.insert("project".into(), "x".repeat(MAX_VALUE_LEN + 1));
    assert!(
        failed()
            .validate(&v, ORIGIN)
            .unwrap_err()
            .contains("longer")
    );

    let mut v = vars();
    let long = format!("{ORIGIN}/{}", "p".repeat(MAX_URL_LEN));
    v.insert("url".into(), long);
    assert!(
        failed()
            .validate(&v, ORIGIN)
            .unwrap_err()
            .contains("longer")
    );
}

#[test]
fn links_must_stay_inside_the_estate() {
    for bad in [
        "https://evil.example/conveyor/runs/1",
        "http://mail.example.test/conveyor/runs/1",
        "https://mail.example.test.evil.example/x",
        "https://mail.example.test@evil.example/x",
        "https://mail.example.test",
        "//mail.example.test/x",
        "/conveyor/runs/1",
        "javascript:alert(1)",
        "https://mail.example.test/x y",
        "https://mail.example.test/x\u{a0}y",
    ] {
        let mut v = vars();
        v.insert("url".into(), bad.into());
        let err = failed().validate(&v, ORIGIN);
        assert!(err.is_err(), "{bad:?} should be refused");
    }
    let mut ok = vars();
    ok.insert("url".into(), format!("{ORIGIN}/a/b?c=d#e"));
    assert!(failed().validate(&ok, ORIGIN).is_ok());
}

fn pairs(v: &BTreeMap<String, String>) -> Vec<(&str, &str)> {
    v.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
}

#[test]
fn every_template_renders_in_every_language() {
    let v = vars();
    let unsubscribe = "https://mail.example.test/gatehouse/ui/unsubscribe?token=abc";
    for template in &TEMPLATES {
        let mut subjects = Vec::new();
        for locale in ["en-US", "pl-PL", "de-DE", "fr-FR", "es-ES"] {
            let mail = template.render(Some(locale), "alice", &pairs(&v), unsubscribe);
            let ctx = format!("{} {locale}", template.id);
            assert!(mail.subject.contains("forge"), "{ctx}");
            for needle in ["alice", "1234", "refs/heads/master", &v["url"], unsubscribe] {
                assert!(mail.text.contains(needle), "{ctx}: {needle}");
            }
            assert!(
                !mail.text.contains('{') && !mail.subject.contains('{'),
                "{ctx}"
            );
            assert!(
                mail.html.contains(&format!("<a href=\"{}\">", v["url"])),
                "{ctx}"
            );
            assert!(
                mail.html.contains(&format!("<a href=\"{unsubscribe}\">")),
                "{ctx}"
            );
            subjects.push(mail.subject);
        }
        subjects.sort();
        subjects.dedup();
        assert_eq!(
            subjects.len(),
            5,
            "{}: one subject per language",
            template.id
        );
    }
}

#[test]
fn unknown_locales_get_english_and_the_two_kinds_differ() {
    let v = vars();
    let en = failed().render(Some("en"), "a", &pairs(&v), "u");
    assert_eq!(failed().render(Some("xx"), "a", &pairs(&v), "u"), en);
    assert_eq!(failed().render(None, "a", &pairs(&v), "u"), en);
    assert_ne!(
        succeeded().render(None, "a", &pairs(&v), "u").subject,
        en.subject
    );
}

#[test]
fn values_are_escaped_in_html_and_never_re_read_as_placeholders() {
    let mut v = vars();
    v.insert("project".into(), "<b>{username}</b> & \"co\"".into());
    let mail = failed().render(
        Some("en"),
        "alice",
        &pairs(&v),
        "https://mail.example.test/u",
    );
    assert!(
        mail.text.contains("<b>{username}</b> & \"co\""),
        "verbatim in text: {}",
        mail.text
    );
    assert!(!mail.html.contains("<b>"), "{}", mail.html);
    assert!(
        mail.html
            .contains("&lt;b&gt;{username}&lt;/b&gt; &amp; &quot;co&quot;")
    );
    assert!(
        !mail.text.contains("<b>alice</b>"),
        "a value is not a template"
    );
}

// -- preferences -------------------------------------------------------

#[tokio::test]
async fn with_no_choice_the_templates_default_applies() {
    let db = db().await;
    let prefs = Preferences::new(&db);
    assert!(prefs.is_subscribed("alice", failed()).await.unwrap());
    assert!(!prefs.is_subscribed("alice", succeeded()).await.unwrap());
    let effective = prefs.effective("alice").await.unwrap();
    assert_eq!(effective.len(), TEMPLATES.len());
    assert!(prefs.overrides("alice").await.unwrap().is_empty());
}

#[tokio::test]
async fn a_choice_is_stored_and_only_when_it_differs_from_the_default() {
    let db = db().await;
    let prefs = Preferences::new(&db);

    prefs.set("alice", failed(), false).await.unwrap();
    assert!(!prefs.is_subscribed("alice", failed()).await.unwrap());
    assert_eq!(
        prefs
            .overrides("alice")
            .await
            .unwrap()
            .get("conveyor.run.failed"),
        Some(&false)
    );

    prefs.set("alice", succeeded(), true).await.unwrap();
    assert!(prefs.is_subscribed("alice", succeeded()).await.unwrap());

    // Back to the default: the row goes away rather than lingering as a copy of it.
    prefs.set("alice", failed(), true).await.unwrap();
    prefs.set("alice", succeeded(), false).await.unwrap();
    assert!(prefs.overrides("alice").await.unwrap().is_empty());
    assert!(prefs.is_subscribed("alice", failed()).await.unwrap());
}

#[tokio::test]
async fn setting_the_same_choice_twice_and_forgetting_a_missing_one_are_fine() {
    let db = db().await;
    let prefs = Preferences::new(&db);
    prefs.set("alice", failed(), false).await.unwrap();
    prefs.set("alice", failed(), false).await.unwrap();
    assert_eq!(prefs.overrides("alice").await.unwrap().len(), 1);
    prefs.set("bob", failed(), true).await.unwrap();
    assert!(prefs.overrides("bob").await.unwrap().is_empty());
}

#[tokio::test]
async fn people_do_not_share_choices() {
    let db = db().await;
    let prefs = Preferences::new(&db);
    prefs.set("alice", failed(), false).await.unwrap();
    assert!(prefs.is_subscribed("bob", failed()).await.unwrap());
    assert!(!prefs.is_subscribed("alice", failed()).await.unwrap());
    // A name that merely contains another's is a different person.
    assert!(prefs.is_subscribed("alice2", failed()).await.unwrap());
    assert!(prefs.is_subscribed("xalice", failed()).await.unwrap());
}

#[tokio::test]
async fn the_effective_list_reflects_the_choices() {
    let db = db().await;
    let prefs = Preferences::new(&db);
    prefs.set("alice", failed(), false).await.unwrap();
    prefs.set("alice", succeeded(), true).await.unwrap();
    let effective = prefs.effective("alice").await.unwrap();
    let get = |id: &str| {
        effective
            .iter()
            .find(|(t, _)| t.id == id)
            .map(|(_, on)| *on)
    };
    assert_eq!(get("conveyor.run.failed"), Some(false));
    assert_eq!(get("conveyor.run.succeeded"), Some(true));
}

// -- unsubscribe tokens -------------------------------------------------

#[test]
fn an_unsubscribe_ticket_round_trips_and_junk_is_not_one() {
    let t = Unsubscribe {
        username: "alice".into(),
        template: "conveyor.run.failed".into(),
    };
    assert_eq!(Unsubscribe::decode(&t.encode()), Some(t));
    for junk in ["", "alice", "{}", "nonsense"] {
        assert_eq!(Unsubscribe::decode(junk), None, "{junk:?}");
    }
}

#[tokio::test]
async fn the_unsubscribe_link_is_public_long_lived_and_reusable() {
    let tokens = VerificationTokens::in_memory();
    let link = issue_link(&tokens, &base(), "alice", "conveyor.run.failed")
        .await
        .unwrap();
    assert!(
        link.starts_with("https://mail.example.test/") && link.contains("/unsubscribe?token="),
        "{link}"
    );
    let token = link.split("token=").nth(1).unwrap();
    for _ in 0..3 {
        assert!(
            tokens
                .peek(PURPOSE_UNSUBSCRIBE, token)
                .await
                .unwrap()
                .is_some()
        );
    }
    assert_eq!(UNSUBSCRIBE_TTL_SECS, 90 * 24 * 60 * 60);
}

// -- dispatch ------------------------------------------------------------

struct World {
    db: Db,
    tokens: VerificationTokens,
    limiter: RateLimiter,
    recorder: Arc<RecordingSender>,
    sender: Arc<dyn Sender>,
}

impl World {
    async fn new() -> Self {
        let recorder = Arc::new(RecordingSender::default());
        let sender: Arc<dyn Sender> = recorder.clone();
        Self {
            db: db().await,
            tokens: VerificationTokens::in_memory(),
            limiter: RateLimiter::in_memory(),
            recorder,
            sender,
        }
    }

    async fn user(&self, name: &str, address: Option<&str>, confirmed: bool) {
        realm::create(
            &self.db,
            &catalog_for_realm(),
            true,
            name,
            "pw",
            vec![Role::User],
            Permissions::new(),
            address.map(str::to_string),
        )
        .await
        .expect("seed");
        if confirmed {
            realm::mark_email_verified(&self.db, name).await.unwrap();
        }
    }

    async fn notify(&self, request: &NotifyRequest) -> Result<Outcome, Invalid> {
        self.notify_via(&*self.sender, request).await
    }

    async fn notify_via(
        &self,
        sender: &dyn Sender,
        request: &NotifyRequest,
    ) -> Result<Outcome, Invalid> {
        let base = base();
        dispatch(
            &Context {
                db: &self.db,
                mailer: sender,
                tokens: &self.tokens,
                base: &base,
                limiter: &self.limiter,
            },
            request,
        )
        .await
    }
}

fn request(username: &str, template: &str) -> NotifyRequest {
    NotifyRequest {
        username: username.into(),
        template: template.into(),
        vars: vars(),
        dedupe_key: None,
        requested: false,
    }
}

#[tokio::test]
async fn a_subscribed_person_with_a_confirmed_address_is_emailed() {
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    let outcome = w
        .notify(&request("alice", "conveyor.run.failed"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Accepted);

    let sent = w.recorder.sent_of("notification");
    assert_eq!(sent.len(), 1, "{:?}", w.recorder.sent());
    assert_eq!(sent[0].to, "alice@example.test");
    assert_eq!(sent[0].username, "alice");
    assert_eq!(sent[0].detail, "conveyor.run.failed");
    assert!(
        sent[0].link.contains("/unsubscribe?token="),
        "{}",
        sent[0].link
    );
}

#[tokio::test]
async fn the_message_follows_the_persons_saved_language() {
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    realm::update(
        &w.db,
        &catalog_for_realm(),
        &SessionDb::init(quench_cache::CacheStore::in_memory()),
        "alice",
        true,
        "alice",
        realm::UserChanges {
            preferred_locale: Some("pl-PL".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    w.notify(&request("alice", "conveyor.run.failed"))
        .await
        .unwrap();
    assert_eq!(w.recorder.sent()[0].locale.as_deref(), Some("pl-PL"));
}

#[tokio::test]
async fn unknown_accounts_and_unconfirmed_or_missing_addresses_are_skipped() {
    let w = World::new().await;
    w.user("unconfirmed", Some("u@example.test"), false).await;
    w.user("bare", None, false).await;
    for (name, expected) in [
        ("ghost", Skip::NoSuchUser),
        ("unconfirmed", Skip::NoVerifiedEmail),
        ("bare", Skip::NoVerifiedEmail),
    ] {
        let outcome = w
            .notify(&request(name, "conveyor.run.failed"))
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Skipped(expected), "{name}");
    }
    assert!(w.recorder.sent().is_empty());
}

#[tokio::test]
async fn a_default_off_kind_is_not_sent_until_the_person_subscribes() {
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    let outcome = w
        .notify(&request("alice", "conveyor.run.succeeded"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Skipped(Skip::NotSubscribed));

    Preferences::new(&w.db)
        .set("alice", succeeded(), true)
        .await
        .unwrap();
    let outcome = w
        .notify(&request("alice", "conveyor.run.succeeded"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Accepted);
    assert_eq!(w.recorder.sent_of("notification").len(), 1);
}

#[tokio::test]
async fn a_requested_notification_goes_out_though_the_kind_is_off_by_default() {
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    let mut req = request("alice", "conveyor.run.succeeded");
    req.requested = true;
    assert_eq!(w.notify(&req).await.unwrap(), Outcome::Accepted);
    assert_eq!(w.recorder.sent_of("notification").len(), 1);
}

#[tokio::test]
async fn an_explicit_opt_out_beats_a_requested_notification() {
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    Preferences::new(&w.db)
        .set("alice", failed(), false)
        .await
        .unwrap();
    let mut req = request("alice", "conveyor.run.failed");
    req.requested = true;
    assert_eq!(
        w.notify(&req).await.unwrap(),
        Outcome::Skipped(Skip::NotSubscribed)
    );
    assert!(w.recorder.sent().is_empty());
}

#[tokio::test]
async fn the_recovered_kind_is_off_by_default_and_renders() {
    let recovered = catalog::find("conveyor.run.recovered").unwrap();
    assert!(!recovered.default_on);
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    assert_eq!(
        w.notify(&request("alice", "conveyor.run.recovered"))
            .await
            .unwrap(),
        Outcome::Skipped(Skip::NotSubscribed)
    );
}

#[tokio::test]
async fn unsubscribing_from_a_default_on_kind_stops_it() {
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    Preferences::new(&w.db)
        .set("alice", failed(), false)
        .await
        .unwrap();
    let outcome = w
        .notify(&request("alice", "conveyor.run.failed"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Skipped(Skip::NotSubscribed));
    assert!(w.recorder.sent().is_empty());
}

#[tokio::test]
async fn declining_costs_nothing_a_skipped_message_spends_no_dedupe_key_or_limit() {
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    let mut req = request("alice", "conveyor.run.succeeded");
    req.dedupe_key = Some("run-1".into());

    for _ in 0..(policy::NOTIFY_TEMPLATE.max + 5) {
        let outcome = w.notify(&req).await.unwrap();
        assert_eq!(outcome, Outcome::Skipped(Skip::NotSubscribed));
    }
    Preferences::new(&w.db)
        .set("alice", succeeded(), true)
        .await
        .unwrap();
    // Neither the dedupe key nor the hourly limit was used up by the refusals.
    assert_eq!(w.notify(&req).await.unwrap(), Outcome::Accepted);
}

#[tokio::test]
async fn the_same_dedupe_key_is_sent_once() {
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    let mut req = request("alice", "conveyor.run.failed");
    req.dedupe_key = Some("run-1234".into());
    assert_eq!(w.notify(&req).await.unwrap(), Outcome::Accepted);
    assert_eq!(
        w.notify(&req).await.unwrap(),
        Outcome::Skipped(Skip::Duplicate)
    );
    req.dedupe_key = Some("run-1235".into());
    assert_eq!(w.notify(&req).await.unwrap(), Outcome::Accepted);
    assert_eq!(w.recorder.sent_of("notification").len(), 2);

    // Another person, or another kind, with the same key is not a duplicate.
    w.user("bob", Some("bob@example.test"), true).await;
    let mut other = request("bob", "conveyor.run.failed");
    other.dedupe_key = Some("run-1235".into());
    assert_eq!(w.notify(&other).await.unwrap(), Outcome::Accepted);
}

#[tokio::test]
async fn one_kind_for_one_person_is_limited_per_hour() {
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    let limit = policy::NOTIFY_TEMPLATE.max;
    for n in 0..limit {
        let outcome = w
            .notify(&request("alice", "conveyor.run.failed"))
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Accepted, "notification {n}");
    }
    let outcome = w
        .notify(&request("alice", "conveyor.run.failed"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Skipped(Skip::RateLimited));
    assert_eq!(w.recorder.sent_of("notification").len(), limit);

    // Someone else is unaffected.
    w.user("bob", Some("bob@example.test"), true).await;
    assert_eq!(
        w.notify(&request("bob", "conveyor.run.failed"))
            .await
            .unwrap(),
        Outcome::Accepted
    );
}

#[tokio::test]
async fn one_person_is_limited_across_kinds_too() {
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    Preferences::new(&w.db)
        .set("alice", succeeded(), true)
        .await
        .unwrap();
    let mut sent = 0;
    for n in 0..(policy::NOTIFY_USER.max + 5) {
        // Alternate kinds so neither per-kind limit is what stops it.
        let template = if n % 2 == 0 {
            "conveyor.run.failed"
        } else {
            "conveyor.run.succeeded"
        };
        if w.notify(&request("alice", template)).await.unwrap() == Outcome::Accepted {
            sent += 1;
        }
    }
    assert_eq!(sent, policy::NOTIFY_USER.max);
}

#[tokio::test]
async fn a_bad_request_is_the_callers_error_and_reveals_nothing_about_accounts() {
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;

    let unknown = w.notify(&request("alice", "conveyor.run.exploded")).await;
    assert!(unknown.unwrap_err().0.contains("no such notification"));

    let mut missing = request("alice", "conveyor.run.failed");
    missing.vars.remove("project");
    assert!(w.notify(&missing).await.is_err());

    let mut elsewhere = request("alice", "conveyor.run.failed");
    elsewhere
        .vars
        .insert("url".into(), "https://evil.example/x".into());
    assert!(w.notify(&elsewhere).await.is_err());

    // Validation happens before any lookup: a bad request about a stranger is
    // refused exactly like one about a real person.
    let mut about_ghost = request("ghost", "conveyor.run.failed");
    about_ghost.vars.remove("run");
    assert!(w.notify(&about_ghost).await.is_err());
    assert!(w.recorder.sent().is_empty());
}

#[tokio::test]
async fn a_failing_mail_server_is_reported_with_whether_to_retry() {
    struct Failing(bool);
    #[async_trait::async_trait]
    impl Sender for Failing {
        async fn send(&self, _: &Recipient<'_>, _: &Mail<'_>) -> Result<(), SendError> {
            Err(SendError::new("smtp down", self.0))
        }
    }
    let w = World::new().await;
    w.user("alice", Some("alice@example.test"), true).await;
    for transient in [true, false] {
        let mut req = request("alice", "conveyor.run.failed");
        req.dedupe_key = Some(format!("k{transient}"));
        let outcome = w.notify_via(&Failing(transient), &req).await.unwrap();
        assert_eq!(
            outcome,
            Outcome::Failed {
                message: "smtp down".into(),
                transient
            }
        );
    }
}

// -- what actually goes out over SMTP -------------------------------------

async fn fake_smtp() -> (u16, Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let received = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = received.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut tcp, _)) = listener.accept().await else {
                return;
            };
            let log = log.clone();
            tokio::spawn(async move {
                let (read, mut write) = tcp.split();
                let mut read = BufReader::new(read);
                let _ = write.write_all(b"220 fake ESMTP\r\n").await;
                let mut line = String::new();
                loop {
                    line.clear();
                    if read.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    let upper = line.to_ascii_uppercase();
                    let reply: &[u8] = if upper.starts_with("EHLO") {
                        b"250-fake\r\n250 8BITMIME\r\n"
                    } else if upper.starts_with("DATA") {
                        let _ = write.write_all(b"354 go\r\n").await;
                        let mut data = String::new();
                        loop {
                            let mut l = String::new();
                            if read.read_line(&mut l).await.unwrap_or(0) == 0 {
                                return;
                            }
                            if l == ".\r\n" {
                                break;
                            }
                            data.push_str(&l);
                        }
                        log.lock().unwrap().push(data);
                        b"250 2.0.0 queued\r\n"
                    } else if upper.starts_with("QUIT") {
                        let _ = write.write_all(b"221 bye\r\n").await;
                        return;
                    } else {
                        b"250 ok\r\n"
                    };
                    let _ = write.write_all(reply).await;
                }
            });
        }
    });
    (port, received)
}

fn smtp_sender(port: u16) -> SmtpSender {
    SmtpSender::new(SmtpConfig {
        host: "127.0.0.1".into(),
        port: Some(port),
        tls: TlsMode::None,
        tls_server_name: None,
        credentials: None,
        from: quench_mail::Mailbox::new(
            Some("Forge".into()),
            quench_mail::Address::new("noreply", "example.test").unwrap(),
        ),
        timeout: Duration::from_secs(3),
    })
    .unwrap()
}

#[tokio::test]
async fn a_notification_goes_out_with_the_standard_one_click_unsubscribe_header() {
    let (port, received) = fake_smtp().await;
    let sender = smtp_sender(port);
    let v = vars();
    let unsubscribe = "https://mail.example.test/gatehouse/ui/unsubscribe?token=tok-123";
    sender
        .send(
            &Recipient {
                address: "alice@example.test",
                username: "alice",
                locale: None,
            },
            &Mail::Notification {
                template: "conveyor.run.failed",
                vars: &pairs(&v),
                unsubscribe,
            },
        )
        .await
        .expect("delivered");

    let mails = received.lock().unwrap();
    let mail = &mails[0];
    assert!(
        mail.contains(&format!("List-Unsubscribe: <{unsubscribe}>\r\n")),
        "{mail}"
    );
    assert!(mail.contains("List-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n"));
    assert!(mail.contains("Subject: Run failed: forge\r\n"));
    assert!(mail.contains("Auto-Submitted: auto-generated"));
    assert!(mail.contains(unsubscribe), "the footer link");
    assert!(mail.contains("https://mail.example.test/conveyor/runs/1234"));
}

#[tokio::test]
async fn an_unknown_template_reaching_the_sender_is_a_permanent_failure() {
    let (port, received) = fake_smtp().await;
    let err = smtp_sender(port)
        .send(
            &Recipient {
                address: "alice@example.test",
                username: "alice",
                locale: None,
            },
            &Mail::Notification {
                template: "made.up.thing",
                vars: &[],
                unsubscribe: "https://x.test/u",
            },
        )
        .await
        .unwrap_err();
    assert!(!err.is_transient());
    assert!(received.lock().unwrap().is_empty());
}

// -- the endpoint -----------------------------------------------------------

async fn app(
    db: Db,
    sender: Arc<dyn Sender>,
    config: JwtConfig,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::api::notify::register_routes();
    let container = ContainerBuilder::new()
        .provide(config)
        .provide(db)
        .provide(sender)
        .provide(base())
        .provide(RateLimiter::in_memory())
        .provide_arc(Arc::new(VerificationTokens::in_memory()))
        .provide_arc(SessionDb::init(quench_cache::CacheStore::in_memory()))
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn post(
    body: serde_json::Value,
    token: Option<&str>,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    if let Some(token) = token {
        headers.insert("authorization", token.parse().unwrap());
    }
    Request::new(
        Method::POST,
        "/api/v1/notify".parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(body.to_string())),
        container.clone(),
    )
}

async fn json(resp: quench_http::response::Response) -> serde_json::Value {
    let bytes = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

fn body(username: &str, template: &str) -> serde_json::Value {
    serde_json::json!({ "username": username, "template": template, "vars": vars() })
}

async fn endpoint_world() -> (
    Arc<RecordingSender>,
    Arc<dyn Endpoint>,
    Arc<quench_http::di::Container>,
    Db,
) {
    let db = db().await;
    let recorder = Arc::new(RecordingSender::default());
    let sender: Arc<dyn Sender> = recorder.clone();
    let (app, container) = app(db.clone(), sender, JwtConfig::for_tests()).await;
    realm::create(
        &db,
        &catalog_for_realm(),
        true,
        "alice",
        "pw",
        vec![Role::User],
        Permissions::new(),
        Some("alice@example.test".into()),
    )
    .await
    .unwrap();
    realm::mark_email_verified(&db, "alice").await.unwrap();
    (recorder, app, container, db)
}

#[tokio::test]
async fn the_endpoint_accepts_a_notification() {
    let _guard = gatehouse_service::test_support::auth_disabled_guard().await;
    let (recorder, app, container, _db) = endpoint_world().await;
    let resp = app
        .call(post(body("alice", "conveyor.run.failed"), None, &container))
        .await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let json = json(resp).await;
    assert_eq!(json["status"], "accepted");
    assert!(json.get("reason").is_none());
    assert_eq!(recorder.sent_of("notification").len(), 1);
}

#[tokio::test]
async fn the_endpoint_says_why_it_skipped() {
    let _guard = gatehouse_service::test_support::auth_disabled_guard().await;
    let (recorder, app, container, _db) = endpoint_world().await;
    for (username, template, reason) in [
        ("ghost", "conveyor.run.failed", "no_such_user"),
        ("alice", "conveyor.run.succeeded", "not_subscribed"),
    ] {
        let resp = app
            .call(post(body(username, template), None, &container))
            .await;
        assert_eq!(resp.status(), StatusCode::OK, "{username} {template}");
        let json = json(resp).await;
        assert_eq!(json["status"], "skipped");
        assert_eq!(json["reason"], reason);
    }
    assert!(recorder.sent().is_empty());
}

#[tokio::test]
async fn the_endpoint_rejects_a_malformed_request_with_the_reason() {
    let _guard = gatehouse_service::test_support::auth_disabled_guard().await;
    let (recorder, app, container, _db) = endpoint_world().await;
    for bad in [
        serde_json::json!({ "username": "alice", "template": "no.such.thing", "vars": vars() }),
        serde_json::json!({ "username": "alice", "template": "conveyor.run.failed", "vars": {} }),
        serde_json::json!({
            "username": "alice", "template": "conveyor.run.failed",
            "vars": { "project": "p", "run": "1", "ref": "r", "url": "https://evil.example/x" }
        }),
    ] {
        let resp = app.call(post(bad, None, &container)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(
            json(resp).await["error"]
                .as_str()
                .is_some_and(|e| !e.is_empty())
        );
    }
    let resp = app
        .call(post(serde_json::json!({ "nonsense": 1 }), None, &container))
        .await;
    assert!(resp.status().is_client_error());
    assert!(recorder.sent().is_empty());
}

#[tokio::test]
async fn the_endpoint_reports_a_mail_failure_as_a_bad_gateway_with_retryable() {
    struct Failing;
    #[async_trait::async_trait]
    impl Sender for Failing {
        async fn send(&self, _: &Recipient<'_>, _: &Mail<'_>) -> Result<(), SendError> {
            Err(SendError::transient("smtp down"))
        }
    }
    let _guard = gatehouse_service::test_support::auth_disabled_guard().await;
    let db = db().await;
    let (app, container) = app(db.clone(), Arc::new(Failing), JwtConfig::for_tests()).await;
    realm::create(
        &db,
        &catalog_for_realm(),
        true,
        "alice",
        "pw",
        vec![Role::User],
        Permissions::new(),
        Some("alice@example.test".into()),
    )
    .await
    .unwrap();
    realm::mark_email_verified(&db, "alice").await.unwrap();
    let resp = app
        .call(post(body("alice", "conveyor.run.failed"), None, &container))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    let json = json(resp).await;
    assert_eq!(json["status"], "failed");
    assert_eq!(json["retryable"], true);
}

#[tokio::test]
async fn a_retry_with_the_same_dedupe_key_is_not_sent_twice() {
    let _guard = gatehouse_service::test_support::auth_disabled_guard().await;
    let (recorder, app, container, _db) = endpoint_world().await;
    let mut request = body("alice", "conveyor.run.failed");
    request["dedupe_key"] = serde_json::json!("run-1234");
    let first = app.call(post(request.clone(), None, &container)).await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let second = app.call(post(request, None, &container)).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(json(second).await["reason"], "duplicate");
    assert_eq!(recorder.sent_of("notification").len(), 1);
}

async fn token_with_scope(config: &JwtConfig, scope: &str) -> String {
    let claims = Claims::for_audiences(
        "some-service".to_string(),
        vec![config.service_name.clone()],
        scope.to_string(),
        None,
        3600,
    );
    format!(
        "Bearer {}",
        config.encode_claims(&claims).await.expect("encode")
    )
}

async fn authed(scope: Option<&str>) -> StatusCode {
    let _guard = service_auth_env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };
    let config = JwtConfig::for_tests_with_signing();
    let token = match scope {
        Some(scope) => Some(token_with_scope(&config, scope).await),
        None => None,
    };
    let db = db().await;
    let recorder = Arc::new(RecordingSender::default());
    let sender: Arc<dyn Sender> = recorder;
    let (app, container) = app(db, sender, config).await;
    app.call(post(
        body("nobody", "conveyor.run.failed"),
        token.as_deref(),
        &container,
    ))
    .await
    .status()
}

#[tokio::test]
async fn with_auth_on_no_token_is_unauthorized() {
    assert_eq!(authed(None).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_token_without_the_notify_grant_is_forbidden() {
    for scope in [
        "gatehouse:read-users",
        "gatehouse:edit-user",
        "sage:write",
        "user",
    ] {
        assert_eq!(authed(Some(scope)).await, StatusCode::FORBIDDEN, "{scope}");
    }
}

#[tokio::test]
async fn the_notify_grant_a_service_role_and_an_admin_may_send() {
    for scope in ["gatehouse:notify", "service", "admin"] {
        // A skipped result (nobody exists) is a 200: what matters is getting past the gate.
        assert_eq!(authed(Some(scope)).await, StatusCode::OK, "{scope}");
    }
}
