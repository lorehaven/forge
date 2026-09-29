use gatehouse_service::PublicBase;
use gatehouse_service::email::{Mail, Recipient, SendError, Sender};
use gatehouse_service::email_change::{EMAIL_CHANGE_TTL_SECS, Ticket, issue_link};
use gatehouse_service::realm::{self, RealmError};
use gatehouse_service::test_support::RecordingSender;
use gatehouse_service::tokens::{PURPOSE_EMAIL_CHANGE, PURPOSE_VERIFY_EMAIL, VerificationTokens};
use gatehouse_service::ui::locale::BrowserLocale;
use gatehouse_service::ui::pages::confirm_email::{
    ConfirmForm, ConfirmQuery, confirm_email_page, confirm_email_submit, render_confirm_email_page,
};
use http::StatusCode;
use http_body_util::BodyExt;
use quench_auth::domain::auth::{Permissions, Role, User};
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Query};
use std::sync::Arc;

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn catalog() -> gatehouse_service::catalog::PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("email-change-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    let result =
        gatehouse_service::catalog::PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn location(resp: quench_http::response::Response) -> String {
    resp.into_hyper()
        .headers()
        .get("location")
        .expect("location header")
        .to_str()
        .unwrap()
        .to_string()
}

async fn body(resp: quench_http::response::Response) -> String {
    let bytes = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// `alice`, optionally with an address, optionally confirmed.
async fn alice(db: &Db, address: Option<&str>, confirmed: bool) -> User {
    let user = realm::create(
        db,
        &catalog(),
        true,
        "alice",
        "correct-horse",
        vec![Role::User],
        Permissions::new(),
        address.map(str::to_string),
    )
    .await
    .expect("seed");
    if confirmed {
        realm::mark_email_verified(db, "alice")
            .await
            .expect("verify");
    }
    user
}

fn ticket(new_email: &str) -> Ticket {
    Ticket {
        username: "alice".into(),
        new_email: new_email.into(),
    }
}

// -- Ticket and the link --------------------------------------------

#[test]
fn a_ticket_survives_encoding_and_garbage_is_not_a_ticket() {
    let t = ticket("new@example.test");
    assert_eq!(Ticket::decode(&t.encode()), Some(t));
    for junk in [
        "",
        "alice",
        "{}",
        "{\"username\":\"a\"}",
        "not json",
        "[1,2]",
    ] {
        assert_eq!(Ticket::decode(junk), None, "{junk:?}");
    }
    // Awkward characters are carried intact, not interpreted.
    let odd = Ticket {
        username: "a\"b\\c\nd".into(),
        new_email: "we'ird+tag@example.test".into(),
    };
    assert_eq!(Ticket::decode(&odd.encode()), Some(odd));
}

#[tokio::test]
async fn the_link_points_at_the_public_origin_and_carries_a_day_long_token() {
    let tokens = VerificationTokens::in_memory();
    let base = PublicBase::resolve("https://mail.example.test", "");
    let t = ticket("new@example.test");
    let link = issue_link(&tokens, &base, &t).await.unwrap();
    assert!(
        link.starts_with("https://mail.example.test/") && link.contains("/confirm-email?token="),
        "{link}"
    );
    let token = link.split("token=").nth(1).unwrap();
    let raw = tokens
        .redeem(PURPOSE_EMAIL_CHANGE, token)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(Ticket::decode(&raw), Some(t));
    assert_eq!(EMAIL_CHANGE_TTL_SECS, 24 * 60 * 60);
}

#[tokio::test]
async fn peeking_reads_a_token_without_spending_it() {
    let tokens = VerificationTokens::in_memory();
    let token = tokens
        .issue(PURPOSE_EMAIL_CHANGE, "value", 60)
        .await
        .unwrap();
    for _ in 0..3 {
        assert_eq!(
            tokens
                .peek(PURPOSE_EMAIL_CHANGE, &token)
                .await
                .unwrap()
                .as_deref(),
            Some("value")
        );
    }
    assert_eq!(
        tokens
            .redeem(PURPOSE_EMAIL_CHANGE, &token)
            .await
            .unwrap()
            .as_deref(),
        Some("value"),
        "still there to be redeemed"
    );
    assert_eq!(
        tokens.peek(PURPOSE_EMAIL_CHANGE, &token).await.unwrap(),
        None
    );
    // And purposes stay separate for peeking too.
    let other = tokens.issue(PURPOSE_VERIFY_EMAIL, "v", 60).await.unwrap();
    assert_eq!(
        tokens.peek(PURPOSE_EMAIL_CHANGE, &other).await.unwrap(),
        None
    );
}

// -- realm::check_password / change_email ---------------------------

#[tokio::test]
async fn check_password_returns_the_user_or_refuses() {
    let db = db().await;
    alice(&db, None, false).await;
    assert!(
        realm::check_password(&db, "alice", "correct-horse")
            .await
            .is_ok()
    );
    let err = realm::check_password(&db, "alice", "wrong")
        .await
        .unwrap_err();
    assert!(matches!(err, RealmError::CurrentPasswordInvalid));
    let err = realm::check_password(&db, "nobody", "x").await.unwrap_err();
    assert!(matches!(err, RealmError::NotFound));
}

#[tokio::test]
async fn wrong_passwords_at_the_gate_count_toward_the_lockout() {
    let db = db().await;
    alice(&db, None, false).await;
    for _ in 0..20 {
        let _ = realm::check_password(&db, "alice", "wrong").await;
    }
    let user = realm::get(&db, "alice").await.unwrap();
    assert!(
        user.is_locked(),
        "a hijacked session cannot guess the password forever"
    );
}

#[tokio::test]
async fn changing_the_address_confirms_it_and_names_the_old_confirmed_one() {
    let db = db().await;
    alice(&db, Some("old@example.test"), true).await;
    let change = realm::change_email(&db, "alice", "  new@example.test ")
        .await
        .unwrap();
    assert_eq!(change.user.email.as_deref(), Some("new@example.test"));
    assert!(change.user.email_verified_at.is_some());
    assert_eq!(
        change.previous_confirmed.as_deref(),
        Some("old@example.test")
    );
    let stored = realm::get(&db, "alice").await.unwrap();
    assert_eq!(stored.email.as_deref(), Some("new@example.test"));
}

#[tokio::test]
async fn an_unconfirmed_or_missing_old_address_is_not_warned() {
    for (old, confirmed) in [(Some("old@example.test"), false), (None, false)] {
        let db = db().await;
        alice(&db, old, confirmed).await;
        let change = realm::change_email(&db, "alice", "new@example.test")
            .await
            .unwrap();
        assert_eq!(change.previous_confirmed, None, "{old:?}");
        assert!(change.user.email_verified_at.is_some());
    }
}

#[tokio::test]
async fn re_confirming_the_same_address_warns_nobody() {
    let db = db().await;
    alice(&db, Some("Same@Example.test"), true).await;
    let change = realm::change_email(&db, "alice", "same@example.test")
        .await
        .unwrap();
    assert_eq!(change.previous_confirmed, None);
}

#[tokio::test]
async fn change_email_refuses_bad_addresses_and_unknown_accounts() {
    let db = db().await;
    alice(&db, Some("old@example.test"), true).await;
    for bad in ["", "nonsense", "a b@example.test", "zoë@example.test"] {
        let err = realm::change_email(&db, "alice", bad).await.unwrap_err();
        assert!(matches!(err, RealmError::EmailInvalid), "{bad:?}");
    }
    let err = realm::change_email(&db, "ghost", "x@example.test")
        .await
        .unwrap_err();
    assert!(matches!(err, RealmError::NotFound));
    let stored = realm::get(&db, "alice").await.unwrap();
    assert_eq!(
        stored.email.as_deref(),
        Some("old@example.test"),
        "unchanged"
    );
}

// -- the confirm page ------------------------------------------------

async fn issue(tokens: &VerificationTokens, t: &Ticket) -> String {
    tokens
        .issue(PURPOSE_EMAIL_CHANGE, &t.encode(), EMAIL_CHANGE_TTL_SECS)
        .await
        .unwrap()
}

async fn confirm(
    db: &Db,
    tokens: &Arc<VerificationTokens>,
    sender: Arc<dyn Sender>,
    token: &str,
) -> String {
    let resp = confirm_email_submit(
        Form(ConfirmForm {
            token: token.to_string(),
        }),
        Inject(Arc::new(db.clone())),
        Inject(tokens.clone()),
        Inject(Arc::new(sender)),
        BrowserLocale(None),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    location(resp)
}

fn recording() -> (Arc<RecordingSender>, Arc<dyn Sender>) {
    let recorder = Arc::new(RecordingSender::default());
    let sender: Arc<dyn Sender> = recorder.clone();
    (recorder, sender)
}

#[tokio::test]
async fn opening_the_link_shows_the_change_and_does_not_make_it() {
    let db = db().await;
    alice(&db, Some("old@example.test"), true).await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let token = issue(&tokens, &ticket("new@example.test")).await;

    let resp = confirm_email_page(
        Query(ConfirmQuery {
            token: token.clone(),
        }),
        Inject(tokens.clone()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body(resp).await;
    assert!(html.contains("new@example.test"), "{html}");
    assert!(html.contains(&format!("value=\"{token}\"")));
    assert!(html.contains("ui_confirm_email_submit"));

    // Nothing happened yet: a scanner that fetched the link changed nothing,
    // and the token is still good.
    let stored = realm::get(&db, "alice").await.unwrap();
    assert_eq!(stored.email.as_deref(), Some("old@example.test"));
    assert!(
        tokens
            .peek(PURPOSE_EMAIL_CHANGE, &token)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn opening_an_unknown_or_foreign_link_goes_to_the_login_error() {
    let tokens = Arc::new(VerificationTokens::in_memory());
    let verify = tokens
        .issue(PURPOSE_VERIFY_EMAIL, "alice", 60)
        .await
        .unwrap();
    for token in ["nonsense".to_string(), verify] {
        let resp = confirm_email_page(Query(ConfirmQuery { token }), Inject(tokens.clone())).await;
        assert_eq!(resp.status(), StatusCode::FOUND);
        assert!(location(resp).contains("ui_login_confirm_email_invalid"));
    }
    // A token that is not a ticket at all (wrong shape) is refused too.
    let plain = tokens
        .issue(PURPOSE_EMAIL_CHANGE, "alice", 60)
        .await
        .unwrap();
    let resp = confirm_email_page(Query(ConfirmQuery { token: plain }), Inject(tokens)).await;
    assert!(location(resp).contains("ui_login_confirm_email_invalid"));
}

#[tokio::test]
async fn confirming_changes_the_address_and_warns_the_old_one() {
    let db = db().await;
    alice(&db, Some("old@example.test"), true).await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let (recorder, sender) = recording();
    let token = issue(&tokens, &ticket("new@example.test")).await;

    let target = confirm(&db, &tokens, sender, &token).await;
    assert!(target.contains("email_changed=1"), "{target}");

    let user = realm::get(&db, "alice").await.unwrap();
    assert_eq!(user.email.as_deref(), Some("new@example.test"));
    assert!(user.email_verified_at.is_some());

    let sent = recorder.sent_of("email-changed");
    assert_eq!(sent.len(), 1, "{:?}", recorder.sent());
    assert_eq!(
        sent[0].to, "old@example.test",
        "the OLD address is the one told"
    );
    assert_eq!(sent[0].detail, "new@example.test");
}

#[tokio::test]
async fn a_confirmation_works_once() {
    let db = db().await;
    alice(&db, Some("old@example.test"), true).await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let (recorder, sender) = recording();
    let token = issue(&tokens, &ticket("new@example.test")).await;
    assert!(
        confirm(&db, &tokens, sender.clone(), &token)
            .await
            .contains("email_changed=1")
    );

    let again = confirm(&db, &tokens, sender, &token).await;
    assert!(again.contains("ui_login_confirm_email_invalid"), "{again}");
    assert_eq!(recorder.sent_of("email-changed").len(), 1);
}

#[tokio::test]
async fn nobody_is_warned_when_the_old_address_was_never_confirmed_or_did_not_exist() {
    for (old, confirmed) in [(Some("old@example.test"), false), (None, false)] {
        let db = db().await;
        alice(&db, old, confirmed).await;
        let tokens = Arc::new(VerificationTokens::in_memory());
        let (recorder, sender) = recording();
        let token = issue(&tokens, &ticket("new@example.test")).await;
        let target = confirm(&db, &tokens, sender, &token).await;
        assert!(target.contains("email_changed=1"), "{old:?}: {target}");
        assert!(recorder.sent().is_empty(), "{old:?}: {:?}", recorder.sent());
    }
}

#[tokio::test]
async fn a_failed_warning_email_does_not_undo_the_change() {
    struct Failing;
    #[async_trait::async_trait]
    impl Sender for Failing {
        async fn send(&self, _: &Recipient<'_>, _: &Mail<'_>) -> Result<(), SendError> {
            Err(SendError::transient("smtp down"))
        }
    }
    let db = db().await;
    alice(&db, Some("old@example.test"), true).await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let token = issue(&tokens, &ticket("new@example.test")).await;
    let target = confirm(&db, &tokens, Arc::new(Failing), &token).await;
    assert!(target.contains("email_changed=1"));
    assert_eq!(
        realm::get(&db, "alice").await.unwrap().email.as_deref(),
        Some("new@example.test")
    );
}

#[tokio::test]
async fn a_ticket_for_a_deleted_account_or_a_garbled_one_is_refused() {
    let db = db().await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let (recorder, sender) = recording();

    let ghost = issue(
        &tokens,
        &Ticket {
            username: "ghost".into(),
            new_email: "x@example.test".into(),
        },
    )
    .await;
    assert!(
        confirm(&db, &tokens, sender.clone(), &ghost)
            .await
            .contains("ui_login_confirm_email_invalid")
    );

    let garbled = tokens
        .issue(PURPOSE_EMAIL_CHANGE, "alice", 60)
        .await
        .unwrap();
    assert!(
        confirm(&db, &tokens, sender, &garbled)
            .await
            .contains("ui_login_confirm_email_invalid")
    );
    assert!(recorder.sent().is_empty());
}

/// Real addresses cannot contain markup (`quench_mail::Address` refuses `<`), so
/// nothing reaches this page today - but text on a page should be escaped whatever
/// it is. That is the page renderer's job (`quench-web`), fixed from 0.1.13.
#[tokio::test]
async fn the_confirm_page_escapes_the_address() {
    let resp = render_confirm_email_page("tok", "<script>alert(1)</script>@example.test");
    let html = body(resp).await;
    assert!(!html.contains("<script>alert"), "{html}");
}
