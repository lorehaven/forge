use gatehouse_service::email::{Mail, Sender};
use gatehouse_service::notices::{notice_address, notify, notify_username};
use gatehouse_service::realm;
use gatehouse_service::test_support::RecordingSender;
use quench_auth::domain::auth::{Permissions, Role, User};
use quench_db::prelude::Db;
use std::sync::Arc;

fn user(email: Option<&str>, confirmed: bool, locale: Option<&str>) -> User {
    let mut user = User::new(
        "alice".to_string(),
        "pw".to_string(),
        vec![Role::User],
        Permissions::new(),
        email.map(str::to_string),
    )
    .unwrap();
    if confirmed {
        user.email_verified_at = Some(chrono::Utc::now());
    }
    user.preferred_locale = locale.map(str::to_string);
    user
}

fn recorder() -> (Arc<RecordingSender>, Arc<dyn Sender>) {
    let recorder = Arc::new(RecordingSender::default());
    let sender: Arc<dyn Sender> = recorder.clone();
    (recorder, sender)
}

#[test]
fn only_a_confirmed_address_may_receive_a_notice() {
    assert_eq!(
        notice_address(&user(Some("a@example.test"), true, None)),
        Some("a@example.test")
    );
    assert_eq!(
        notice_address(&user(Some("  a@example.test  "), true, None)),
        Some("a@example.test")
    );
    assert_eq!(
        notice_address(&user(Some("a@example.test"), false, None)),
        None
    );
    assert_eq!(notice_address(&user(None, true, None)), None);
    assert_eq!(notice_address(&user(Some("   "), true, None)), None);
}

#[tokio::test]
async fn a_notice_goes_to_the_confirmed_address_in_the_saved_language() {
    let (recorder, sender) = recorder();
    notify(
        &*sender,
        &user(Some("a@example.test"), true, Some("pl-PL")),
        &Mail::PasswordChanged,
        Some("de-DE"),
    )
    .await;
    let sent = recorder.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, "a@example.test");
    assert_eq!(
        sent[0].locale.as_deref(),
        Some("pl-PL"),
        "saved beats the browser's"
    );
}

#[tokio::test]
async fn the_browsers_language_is_the_fallback() {
    let (recorder, sender) = recorder();
    notify(
        &*sender,
        &user(Some("a@example.test"), true, None),
        &Mail::MfaEnabled,
        Some("de-DE"),
    )
    .await;
    assert_eq!(recorder.sent()[0].locale.as_deref(), Some("de-DE"));
}

#[tokio::test]
async fn nothing_is_sent_without_a_confirmed_address() {
    let (recorder, sender) = recorder();
    for account in [
        user(Some("a@example.test"), false, None),
        user(None, false, None),
    ] {
        notify(&*sender, &account, &Mail::PasswordChanged, None).await;
    }
    assert!(recorder.sent().is_empty());
}

#[tokio::test]
async fn a_failing_sender_is_swallowed() {
    struct Failing;
    #[async_trait::async_trait]
    impl Sender for Failing {
        async fn send(
            &self,
            _: &gatehouse_service::email::Recipient<'_>,
            _: &Mail<'_>,
        ) -> Result<(), gatehouse_service::email::SendError> {
            Err(gatehouse_service::email::SendError::transient("down"))
        }
    }
    // Returns normally: a notice that cannot be sent never fails the request.
    notify(
        &Failing,
        &user(Some("a@example.test"), true, None),
        &Mail::PasswordChanged,
        None,
    )
    .await;
}

#[tokio::test]
async fn notify_username_looks_the_account_up_and_ignores_strangers() {
    let db = Db::connect("").await.expect("in-memory db");
    let (recorder, sender) = recorder();
    notify_username(&*sender, &db, "nobody", &Mail::MfaDisabled, None).await;
    assert!(recorder.sent().is_empty());

    let dir = std::env::temp_dir().join(format!("notices-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    let catalog =
        gatehouse_service::catalog::PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    realm::create(
        &db,
        &catalog,
        true,
        "alice",
        "pw",
        vec![Role::User],
        Permissions::new(),
        Some("a@example.test".into()),
    )
    .await
    .unwrap();
    realm::mark_email_verified(&db, "alice").await.unwrap();
    notify_username(&*sender, &db, "alice", &Mail::MfaDisabled, None).await;
    assert_eq!(recorder.sent_of("mfa-disabled").len(), 1);
}
