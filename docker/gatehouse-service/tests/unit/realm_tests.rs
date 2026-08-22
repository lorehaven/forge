use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::realm::*;
use quench_auth::prelude::{Permissions, Role, SessionDb, User};
use quench_cache::CacheStore;
use quench_db::prelude::Db;
use std::sync::Arc;

/// `GATEHOUSE_KEY_ENCRYPTION_KEY` is read (via `mfa`/`crypto`) by the MFA
/// paths; every module in this crate that needs it sets this exact same
/// value and never unsets it - see `crypto::TEST_KEY_MATERIAL`'s doc
/// comment for why that's the one convention safe under this crate's
/// single, parallel `--bin` test binary.
fn with_key() {
    envmnt::set(
        "GATEHOUSE_KEY_ENCRYPTION_KEY",
        gatehouse_service::test_support::TEST_KEY_MATERIAL,
    );
}

fn catalog(toml: &str) -> PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("realm-test-catalog-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, toml).unwrap();
    let result = PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn default_catalog() -> PermissionCatalog {
    catalog(
        r#"
        [services.sage]
        actions = ["read", "write"]

        [templates.viewer]
        sage = ["read"]
        "#,
    )
}

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn sessions() -> Arc<SessionDb> {
    SessionDb::init(CacheStore::in_memory())
}

/// `mfa::totp` is private to that module, so this rebuilds the same TOTP
/// object here to get a code `enable_mfa`/`authenticate_mfa` will accept -
/// same parameters `mfa.rs` uses (SHA1, 6 digits, 30s step, "Forge" issuer).
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

async fn seed_user(db: &Db, username: &str, password: &str, roles: Vec<Role>) -> User {
    create(
        db,
        &default_catalog(),
        true,
        username,
        password,
        roles,
        Permissions::new(),
        None,
    )
    .await
    .expect("seed user")
}

// -- RealmError ----------------------------------------------------

#[test]
fn every_realm_error_has_a_status_message_and_i18n_key() {
    let errors = [
        RealmError::NotFound,
        RealmError::UsernameEmpty,
        RealmError::PasswordEmpty,
        RealmError::AlreadyExists,
        RealmError::UnknownGrants(vec!["sage:delete-everything".to_string()]),
        RealmError::LastAdmin,
        RealmError::SelfDemote,
        RealmError::SelfDelete,
        RealmError::SelfDisable,
        RealmError::UnknownTemplate,
        RealmError::RolesRequireAdmin,
        RealmError::MfaCodeInvalid,
        RealmError::Internal,
    ];
    for error in errors {
        assert!(!error.message().is_empty());
        assert!(!error.i18n_key().is_empty());
        let _ = error.status();
    }
}

#[test]
fn unknown_grants_message_lists_the_offending_grants() {
    let error = RealmError::UnknownGrants(vec!["sage:oops".to_string()]);
    assert!(error.message().contains("sage:oops"));
    assert_eq!(error.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

// -- create ----------------------------------------------------------

#[tokio::test]
async fn create_rejects_an_empty_username() {
    let db = db().await;
    let error = create(
        &db,
        &default_catalog(),
        true,
        "   ",
        "password",
        vec![],
        Permissions::new(),
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RealmError::UsernameEmpty));
}

#[tokio::test]
async fn create_rejects_an_empty_password() {
    let db = db().await;
    let error = create(
        &db,
        &default_catalog(),
        true,
        "alice",
        "",
        vec![],
        Permissions::new(),
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RealmError::PasswordEmpty));
}

#[tokio::test]
async fn create_rejects_a_wildcard_role_from_a_non_admin_actor() {
    let db = db().await;
    let error = create(
        &db,
        &default_catalog(),
        false,
        "alice",
        "password",
        vec![Role::Admin],
        Permissions::new(),
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RealmError::RolesRequireAdmin));
}

#[tokio::test]
async fn create_allows_a_wildcard_role_from_an_admin_actor() {
    let db = db().await;
    let created = create(
        &db,
        &default_catalog(),
        true,
        "alice",
        "password",
        vec![Role::Admin],
        Permissions::new(),
        None,
    )
    .await
    .expect("create");
    assert!(created.get_roles().contains(&Role::Admin));
}

#[tokio::test]
async fn create_defaults_to_role_user_when_none_given() {
    let db = db().await;
    let created = create(
        &db,
        &default_catalog(),
        false,
        "alice",
        "password",
        vec![],
        Permissions::new(),
        None,
    )
    .await
    .expect("create");
    assert_eq!(created.get_roles(), vec![Role::User]);
}

#[tokio::test]
async fn create_rejects_a_duplicate_username() {
    let db = db().await;
    seed_user(&db, "alice", "password", vec![]).await;

    let error = create(
        &db,
        &default_catalog(),
        true,
        "alice",
        "another-password",
        vec![],
        Permissions::new(),
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RealmError::AlreadyExists));
}

#[tokio::test]
async fn create_rejects_grants_the_catalog_does_not_recognize() {
    let db = db().await;
    let mut permissions = Permissions::new();
    permissions.insert("not-a-real-service".to_string(), Default::default());

    let error = create(
        &db,
        &default_catalog(),
        true,
        "alice",
        "password",
        vec![],
        permissions,
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RealmError::UnknownGrants(_)));
}

// -- get / list --------------------------------------------------------

#[tokio::test]
async fn get_returns_not_found_for_a_missing_user() {
    let db = db().await;
    let error = get(&db, "nobody").await.unwrap_err();
    assert!(matches!(error, RealmError::NotFound));
}

#[tokio::test]
async fn list_returns_users_sorted_by_username() {
    let db = db().await;
    seed_user(&db, "zoe", "password", vec![]).await;
    seed_user(&db, "amy", "password", vec![]).await;

    let users = list(&db).await.expect("list");
    let names: Vec<&str> = users.iter().map(|u| u.username.as_str()).collect();
    assert_eq!(names, vec!["amy", "zoe"]);
}

// -- update --------------------------------------------------------

#[tokio::test]
async fn update_rejects_self_demotion_from_admin() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![Role::Admin]).await;

    let error = update(
        &db,
        &default_catalog(),
        &sessions,
        "alice",
        true,
        "alice",
        UserChanges {
            roles: Some(vec![Role::User]),
            ..UserChanges::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RealmError::SelfDemote));
}

#[tokio::test]
async fn update_rejects_demoting_the_last_admin() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![Role::Admin]).await;

    let error = update(
        &db,
        &default_catalog(),
        &sessions,
        "someone-else",
        true,
        "alice",
        UserChanges {
            roles: Some(vec![Role::User]),
            ..UserChanges::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RealmError::LastAdmin));
}

#[tokio::test]
async fn update_allows_demoting_an_admin_when_another_admin_remains() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![Role::Admin]).await;
    seed_user(&db, "bob", "password", vec![Role::Admin]).await;

    let updated = update(
        &db,
        &default_catalog(),
        &sessions,
        "someone-else",
        true,
        "alice",
        UserChanges {
            roles: Some(vec![Role::User]),
            ..UserChanges::default()
        },
    )
    .await
    .expect("update");
    assert_eq!(updated.get_roles(), vec![Role::User]);
}

#[tokio::test]
async fn update_rejects_an_empty_password() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![]).await;

    let error = update(
        &db,
        &default_catalog(),
        &sessions,
        "alice",
        false,
        "alice",
        UserChanges {
            password: Some(String::new()),
            ..UserChanges::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RealmError::PasswordEmpty));
}

#[tokio::test]
async fn update_changes_the_password_and_does_not_end_the_actors_own_session() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![]).await;
    let (_, refresh_token) = sessions
        .create("alice", 3600)
        .await
        .expect("create session");

    let updated = update(
        &db,
        &default_catalog(),
        &sessions,
        "alice",
        false,
        "alice",
        UserChanges {
            password: Some("new-password".to_string()),
            ..UserChanges::default()
        },
    )
    .await
    .expect("update");
    assert!(updated.verify_password("new-password"));
    assert!(
        sessions
            .revoke_by_refresh_token(&refresh_token)
            .await
            .expect("revoke")
    );
}

#[tokio::test]
async fn update_ends_sessions_when_someone_else_changes_the_users_access() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![]).await;
    sessions
        .create("alice", 3600)
        .await
        .expect("create session");

    update(
        &db,
        &default_catalog(),
        &sessions,
        "an-admin",
        true,
        "alice",
        UserChanges {
            permissions: Some(Permissions::new()),
            ..UserChanges::default()
        },
    )
    .await
    .expect("update");

    assert!(
        sessions
            .sessions_for("alice")
            .await
            .expect("sessions")
            .is_empty()
    );
}

#[tokio::test]
async fn update_rejects_unknown_grants() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![]).await;
    let mut permissions = Permissions::new();
    permissions.insert("not-a-real-service".to_string(), Default::default());

    let error = update(
        &db,
        &default_catalog(),
        &sessions,
        "an-admin",
        true,
        "alice",
        UserChanges {
            permissions: Some(permissions),
            ..UserChanges::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RealmError::UnknownGrants(_)));
}

#[tokio::test]
async fn update_updates_profile_fields() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![]).await;

    let updated = update(
        &db,
        &default_catalog(),
        &sessions,
        "alice",
        false,
        "alice",
        UserChanges {
            display_name: Some("Alice A.".to_string()),
            title: Some("Engineer".to_string()),
            ..UserChanges::default()
        },
    )
    .await
    .expect("update");
    assert_eq!(updated.display_name.as_deref(), Some("Alice A."));
    assert_eq!(updated.title.as_deref(), Some("Engineer"));
}

// -- apply_template / register --------------------------------------

#[tokio::test]
async fn apply_template_rejects_an_unknown_template() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![]).await;

    let error = apply_template(
        &db,
        &default_catalog(),
        &sessions,
        "an-admin",
        "alice",
        "no-such-template",
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RealmError::UnknownTemplate));
}

#[tokio::test]
async fn apply_template_replaces_permissions_with_the_named_templates_grants() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![]).await;

    let updated = apply_template(
        &db,
        &default_catalog(),
        &sessions,
        "an-admin",
        "alice",
        "viewer",
    )
    .await
    .expect("apply template");
    let permissions: Permissions =
        serde_json::from_value(updated.permissions).expect("permissions");
    assert!(permissions.contains_key("sage"));
}

#[tokio::test]
async fn register_creates_an_ordinary_user_with_the_given_email() {
    let db = db().await;
    let user = register(
        &db,
        &default_catalog(),
        "alice",
        "password",
        "alice@example.test",
    )
    .await
    .expect("register");
    assert_eq!(user.get_roles(), vec![Role::User]);
    assert_eq!(user.email.as_deref(), Some("alice@example.test"));
}

// -- mark_email_verified / reset_password / delete --------------------

#[tokio::test]
async fn mark_email_verified_stamps_the_verification_time() {
    let db = db().await;
    seed_user(&db, "alice", "password", vec![]).await;

    mark_email_verified(&db, "alice").await.expect("verify");

    let user = get(&db, "alice").await.expect("get");
    assert!(user.email_verified_at.is_some());
}

#[tokio::test]
async fn reset_password_rejects_an_empty_password() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![]).await;

    let error = reset_password(&db, &sessions, "alice", "")
        .await
        .unwrap_err();
    assert!(matches!(error, RealmError::PasswordEmpty));
}

#[tokio::test]
async fn reset_password_changes_the_password_and_ends_every_session() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![]).await;
    sessions
        .create("alice", 3600)
        .await
        .expect("create session");

    reset_password(&db, &sessions, "alice", "brand-new-password")
        .await
        .expect("reset");

    let user = get(&db, "alice").await.expect("get");
    assert!(user.verify_password("brand-new-password"));
    assert!(
        sessions
            .sessions_for("alice")
            .await
            .expect("sessions")
            .is_empty()
    );
}

#[tokio::test]
async fn delete_rejects_deleting_yourself() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![]).await;

    let error = delete(&db, &sessions, "alice", "alice").await.unwrap_err();
    assert!(matches!(error, RealmError::SelfDelete));
}

#[tokio::test]
async fn delete_rejects_deleting_the_last_admin() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![Role::Admin]).await;

    let error = delete(&db, &sessions, "someone-else", "alice")
        .await
        .unwrap_err();
    assert!(matches!(error, RealmError::LastAdmin));
}

#[tokio::test]
async fn delete_removes_the_user_and_ends_their_sessions() {
    let db = db().await;
    let sessions = sessions();
    seed_user(&db, "alice", "password", vec![]).await;
    sessions
        .create("alice", 3600)
        .await
        .expect("create session");

    delete(&db, &sessions, "an-admin", "alice")
        .await
        .expect("delete");

    assert!(matches!(
        get(&db, "alice").await.unwrap_err(),
        RealmError::NotFound
    ));
    assert!(
        sessions
            .sessions_for("alice")
            .await
            .expect("sessions")
            .is_empty()
    );
}

// -- authenticate ------------------------------------------------------

#[tokio::test]
async fn authenticate_reports_not_found_for_an_unknown_user() {
    let db = db().await;
    let outcome = authenticate(&db, "nobody", "password")
        .await
        .expect("authenticate");
    assert!(matches!(outcome, AuthOutcome::NotFound));
}

#[tokio::test]
async fn authenticate_succeeds_with_the_right_password() {
    let db = db().await;
    seed_user(&db, "alice", "correct-password", vec![]).await;

    let outcome = authenticate(&db, "alice", "correct-password")
        .await
        .expect("authenticate");
    assert!(matches!(outcome, AuthOutcome::Success(_)));
}

#[tokio::test]
async fn authenticate_rejects_the_wrong_password() {
    let db = db().await;
    seed_user(&db, "alice", "correct-password", vec![]).await;

    let outcome = authenticate(&db, "alice", "wrong-password")
        .await
        .expect("authenticate");
    assert!(matches!(outcome, AuthOutcome::WrongPassword));
}

#[tokio::test]
async fn authenticate_reports_disabled_accounts() {
    let db = db().await;
    seed_user(&db, "alice", "password", vec![]).await;
    set_disabled(&db, "alice", true).await.expect("disable");

    let outcome = authenticate(&db, "alice", "password")
        .await
        .expect("authenticate");
    assert!(matches!(outcome, AuthOutcome::Disabled));
}

#[tokio::test]
async fn authenticate_requires_mfa_when_enabled() {
    with_key();
    let db = db().await;
    seed_user(&db, "alice", "password", vec![]).await;
    let (secret, _) = begin_mfa_enrollment("alice").expect("begin enrollment");
    let current = current_totp_code(&secret, "alice");
    enable_mfa(&db, "alice", &secret, &current)
        .await
        .expect("enable mfa");

    let outcome = authenticate(&db, "alice", "password")
        .await
        .expect("authenticate");
    assert!(matches!(outcome, AuthOutcome::MfaRequired { .. }));
}

// -- MFA enrollment / authenticate_mfa --------------------------------

#[tokio::test]
async fn enable_mfa_rejects_a_wrong_code() {
    with_key();
    let db = db().await;
    seed_user(&db, "alice", "password", vec![]).await;
    let (secret, _) = begin_mfa_enrollment("alice").expect("begin enrollment");

    let error = enable_mfa(&db, "alice", &secret, "000000")
        .await
        .unwrap_err();
    assert!(matches!(error, RealmError::MfaCodeInvalid));
}

#[tokio::test]
async fn disable_mfa_turns_it_back_off() {
    with_key();
    let db = db().await;
    seed_user(&db, "alice", "password", vec![]).await;

    disable_mfa(&db, "alice").await.expect("disable");
    let user = get(&db, "alice").await.expect("get");
    assert!(!user.mfa_enabled);
    assert!(user.mfa_secret.is_none());
}

#[tokio::test]
async fn authenticate_mfa_rejects_an_invalid_pending_token() {
    let db = db().await;
    let outcome = authenticate_mfa(&db, "not-a-real-token", "123456")
        .await
        .expect("authenticate_mfa");
    assert!(matches!(outcome, AuthOutcome::WrongPassword));
}

// -- set_disabled / unlock ---------------------------------------------

#[tokio::test]
async fn set_disabled_toggles_the_disabled_timestamp() {
    let db = db().await;
    seed_user(&db, "alice", "password", vec![]).await;

    let disabled = set_disabled(&db, "alice", true).await.expect("disable");
    assert!(disabled.disabled_at.is_some());

    let enabled = set_disabled(&db, "alice", false).await.expect("enable");
    assert!(enabled.disabled_at.is_none());
}

#[tokio::test]
async fn unlock_clears_the_lockout_state() {
    let db = db().await;
    seed_user(&db, "alice", "password", vec![]).await;

    // Five wrong attempts trips the default lockout threshold.
    for _ in 0..5 {
        authenticate(&db, "alice", "wrong-password")
            .await
            .expect("authenticate");
    }
    let locked = get(&db, "alice").await.expect("get");
    assert!(locked.is_locked());

    let unlocked = unlock(&db, "alice").await.expect("unlock");
    assert!(!unlocked.is_locked());
    assert_eq!(unlocked.failed_login_attempts, 0);
}
