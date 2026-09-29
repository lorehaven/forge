use gatehouse_service::PublicBase;
use gatehouse_service::email::{Mail, Recipient, SendError, Sender};
use gatehouse_service::invites::{INVITE_TTL_SECS, send_invite, unusable_password};
use gatehouse_service::realm::{self, AuthOutcome, RealmError};
use gatehouse_service::test_support::RecordingSender;
use gatehouse_service::tokens::{PURPOSE_INVITE, PURPOSE_RESET_PASSWORD, VerificationTokens};
use gatehouse_service::ui::pages::invite::{
    AcceptForm, AcceptNotice, AcceptQuery, accept_invite_page, accept_invite_submit,
    render_accept_invite_page,
};
use http::StatusCode;
use http_body_util::BodyExt;
use quench_auth::domain::auth::{Permissions, Role};
use quench_auth::domain::session::SessionDb;
use quench_cache::CacheStore;
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Query};
use std::sync::Arc;

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn catalog() -> gatehouse_service::catalog::PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("invite-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    let result =
        gatehouse_service::catalog::PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn sessions() -> Arc<SessionDb> {
    SessionDb::init(CacheStore::in_memory())
}

fn base() -> PublicBase {
    PublicBase::resolve("https://mail.example.test", "")
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

async fn invited(db: &Db, name: &str, email: &str) -> quench_auth::domain::auth::User {
    realm::create_invited(
        db,
        &catalog(),
        true,
        name,
        vec![Role::User],
        Permissions::new(),
        email,
    )
    .await
    .expect("create invited")
}

// -- realm::create_invited ------------------------------------------

#[tokio::test]
async fn an_invited_account_has_an_unconfirmed_address_and_a_password_nobody_knows() {
    let db = db().await;
    let user = invited(&db, "alice", "alice@example.test").await;
    assert_eq!(user.email.as_deref(), Some("alice@example.test"));
    assert!(user.email_verified_at.is_none());
    for guess in ["", "password", "alice", "changeme", "alice@example.test"] {
        assert!(!user.verify_password(guess), "{guess:?}");
    }
    // Neither the random password nor anything else lets it log in before the
    // invitation is accepted: the address is unconfirmed.
    let outcome = realm::authenticate(&db, "alice", &unusable_password())
        .await
        .unwrap();
    assert!(!matches!(outcome, AuthOutcome::Success(_)));
}

#[tokio::test]
async fn unusable_passwords_are_long_and_never_repeat() {
    let a = unusable_password();
    let b = unusable_password();
    assert!(a.len() >= 64, "{}", a.len());
    assert_ne!(a, b);
}

#[tokio::test]
async fn an_invitation_needs_a_deliverable_address() {
    let db = db().await;
    for (email, expected) in [
        ("", "InviteNeedsEmail"),
        ("   ", "InviteNeedsEmail"),
        ("not-an-address", "EmailInvalid"),
        ("a b@example.test", "EmailInvalid"),
        ("zoë@example.test", "EmailInvalid"),
    ] {
        let err = realm::create_invited(
            &db,
            &catalog(),
            true,
            "someone",
            vec![Role::User],
            Permissions::new(),
            email,
        )
        .await
        .unwrap_err();
        assert!(format!("{err:?}").contains(expected), "{email:?}: {err:?}");
    }
    assert!(
        realm::get(&db, "someone").await.is_err(),
        "no account was created"
    );
}

#[tokio::test]
async fn the_address_is_trimmed_and_the_username_must_be_free() {
    let db = db().await;
    let user = invited(&db, "alice", "  alice@example.test  ").await;
    assert_eq!(user.email.as_deref(), Some("alice@example.test"));
    let err = realm::create_invited(
        &db,
        &catalog(),
        true,
        "alice",
        vec![Role::User],
        Permissions::new(),
        "other@example.test",
    )
    .await
    .unwrap_err();
    assert!(matches!(err, RealmError::AlreadyExists));
}

#[tokio::test]
async fn only_an_admin_may_invite_someone_into_an_admin_role() {
    let db = db().await;
    let err = realm::create_invited(
        &db,
        &catalog(),
        false,
        "sneaky",
        vec![Role::Admin],
        Permissions::new(),
        "s@example.test",
    )
    .await
    .unwrap_err();
    assert!(matches!(err, RealmError::RolesRequireAdmin));
}

#[test]
fn the_new_errors_have_statuses_messages_and_translation_keys() {
    for (err, status, key) in [
        (
            RealmError::EmailInvalid,
            StatusCode::BAD_REQUEST,
            "ui_register_error_email_invalid",
        ),
        (
            RealmError::InviteNeedsEmail,
            StatusCode::BAD_REQUEST,
            "ui_admin_error_invite_needs_email",
        ),
        (
            RealmError::AlreadyConfirmed,
            StatusCode::CONFLICT,
            "ui_admin_error_already_confirmed",
        ),
    ] {
        assert_eq!(err.status(), status);
        assert!(!err.message().is_empty());
        assert_eq!(err.i18n_key(), key);
    }
}

// -- invites::send_invite -------------------------------------------

#[tokio::test]
async fn an_invitation_email_carries_a_working_seven_day_link() {
    let db = db().await;
    let user = invited(&db, "alice", "alice@example.test").await;
    let tokens = VerificationTokens::in_memory();
    let recorder = RecordingSender::default();

    send_invite(&tokens, &recorder, &base(), &user, None)
        .await
        .expect("sent");

    let sent = recorder.sent_of("invite");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, "alice@example.test");
    assert!(
        sent[0].link.starts_with("https://mail.example.test/")
            && sent[0].link.contains("/accept-invite?token="),
        "{}",
        sent[0].link
    );
    let token = sent[0].link.split("token=").nth(1).unwrap();
    let who = tokens.redeem(PURPOSE_INVITE, token).await.unwrap();
    assert_eq!(who.as_deref(), Some("alice"));
    assert_eq!(INVITE_TTL_SECS, 7 * 24 * 60 * 60);
}

#[tokio::test]
async fn the_invitation_follows_the_saved_language_then_the_fallback() {
    let db = db().await;
    let user = invited(&db, "alice", "alice@example.test").await;
    let tokens = VerificationTokens::in_memory();
    let recorder = RecordingSender::default();
    send_invite(&tokens, &recorder, &base(), &user, Some("de-DE"))
        .await
        .unwrap();
    assert_eq!(recorder.sent()[0].locale.as_deref(), Some("de-DE"));
}

#[tokio::test]
async fn an_invitation_needs_an_address_and_reports_a_send_failure() {
    let db = db().await;
    let no_address = realm::create(
        &db,
        &catalog(),
        true,
        "plain",
        "pw",
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .await
    .unwrap();
    let tokens = VerificationTokens::in_memory();
    let recorder = RecordingSender::default();
    let err = send_invite(&tokens, &recorder, &base(), &no_address, None)
        .await
        .unwrap_err();
    assert!(err.contains("no email"), "{err}");
    assert!(recorder.sent().is_empty());

    struct Failing;
    #[async_trait::async_trait]
    impl Sender for Failing {
        async fn send(&self, _: &Recipient<'_>, _: &Mail<'_>) -> Result<(), SendError> {
            Err(SendError::transient("smtp down"))
        }
    }
    let user = invited(&db, "alice", "alice@example.test").await;
    let err = send_invite(&tokens, &Failing, &base(), &user, None)
        .await
        .unwrap_err();
    assert!(err.contains("smtp down"), "{err}");
}

// -- the accept-invite page -----------------------------------------

async fn accept(db: &Db, tokens: &Arc<VerificationTokens>, token: &str, password: &str) -> String {
    let resp = accept_invite_submit(
        Form(AcceptForm {
            token: token.to_string(),
            password: password.to_string(),
        }),
        Inject(Arc::new(db.clone())),
        Inject(sessions()),
        Inject(tokens.clone()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    location(resp)
}

#[tokio::test]
async fn accepting_sets_the_password_confirms_the_address_and_allows_login() {
    let db = db().await;
    invited(&db, "alice", "alice@example.test").await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let token = tokens
        .issue(PURPOSE_INVITE, "alice", INVITE_TTL_SECS)
        .await
        .unwrap();

    let target = accept(&db, &tokens, &token, "my-own-password").await;
    assert!(target.contains("invited=1"), "{target}");

    let user = realm::get(&db, "alice").await.unwrap();
    assert!(
        user.email_verified_at.is_some(),
        "the link reached the address"
    );
    assert!(user.verify_password("my-own-password"));
    let outcome = realm::authenticate(&db, "alice", "my-own-password")
        .await
        .unwrap();
    assert!(matches!(outcome, AuthOutcome::Success(_)));
}

#[tokio::test]
async fn an_invitation_works_once() {
    let db = db().await;
    invited(&db, "alice", "alice@example.test").await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let token = tokens
        .issue(PURPOSE_INVITE, "alice", INVITE_TTL_SECS)
        .await
        .unwrap();
    assert!(
        accept(&db, &tokens, &token, "first-password")
            .await
            .contains("invited=1")
    );
    let again = accept(&db, &tokens, &token, "second-password").await;
    assert!(again.contains("ui_login_invite_invalid"), "{again}");
    let user = realm::get(&db, "alice").await.unwrap();
    assert!(
        user.verify_password("first-password"),
        "the replay changed nothing"
    );
}

#[tokio::test]
async fn a_blank_password_does_not_burn_the_invitation() {
    let db = db().await;
    invited(&db, "alice", "alice@example.test").await;
    let tokens = Arc::new(VerificationTokens::in_memory());
    let token = tokens
        .issue(PURPOSE_INVITE, "alice", INVITE_TTL_SECS)
        .await
        .unwrap();

    for blank in ["", "   "] {
        let target = accept(&db, &tokens, &token, blank).await;
        assert!(target.contains("ui_reset_error_password_empty"), "{target}");
        assert!(
            target.contains(&format!("token={token}")),
            "back to the same form"
        );
    }
    assert!(
        accept(&db, &tokens, &token, "real-password")
            .await
            .contains("invited=1")
    );
}

#[tokio::test]
async fn unknown_and_wrong_purpose_tokens_are_refused() {
    let db = db().await;
    invited(&db, "alice", "alice@example.test").await;
    let tokens = Arc::new(VerificationTokens::in_memory());

    let target = accept(&db, &tokens, "not-a-real-token", "pw").await;
    assert!(target.contains("ui_login_invite_invalid"));

    // A password-reset token must not double as an invitation.
    let reset = tokens
        .issue(PURPOSE_RESET_PASSWORD, "alice", 3600)
        .await
        .unwrap();
    let target = accept(&db, &tokens, &reset, "pw").await;
    assert!(target.contains("ui_login_invite_invalid"), "{target}");
    let user = realm::get(&db, "alice").await.unwrap();
    assert!(user.email_verified_at.is_none());
}

#[tokio::test]
async fn the_page_renders_a_form_carrying_the_token() {
    let resp = accept_invite_page(
        Query(AcceptQuery {
            token: "abc-123".to_string(),
        }),
        Query(AcceptNotice::default()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("accept-invite"), "{html}");
    assert!(html.contains("value=\"abc-123\""));
    assert!(html.contains("ui_invite_submit"));
    assert!(!html.contains("ui_reset_error_password_empty"));
}

#[tokio::test]
async fn the_page_shows_the_blank_password_error_and_only_that_one() {
    for (err, shown) in [
        (Some("ui_reset_error_password_empty"), true),
        (Some("<script>alert(1)</script>"), false),
        (None, false),
    ] {
        let resp = render_accept_invite_page(
            "tok",
            &AcceptNotice {
                err: err.map(str::to_string),
            },
        );
        let body = resp
            .into_hyper()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert_eq!(
            html.contains("ui_reset_error_password_empty"),
            shown,
            "{err:?}"
        );
        assert!(!html.contains("<script>alert"), "nothing reflected");
    }
}
