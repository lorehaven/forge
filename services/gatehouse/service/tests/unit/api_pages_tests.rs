//! What the browser pages draw from: the dashboard, the account, the enrolment, the administration, the links of emails.

use gatehouse_core::{
    Account, AdminUser, AdminUsers, ConfirmEmail, Dashboard, MfaEnrollment, UnsubscribeInfo,
};
use gatehouse_service::PublicBase;
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::notify::unsubscribe::issue_link;
use gatehouse_service::realm;
use gatehouse_service::test_support::{RecordingSender, TEST_KEY_MATERIAL, service_auth_env_lock};
use gatehouse_service::tokens::{PURPOSE_EMAIL_CHANGE, VerificationTokens};
use quench_auth::domain::auth::{Permissions, Role, User, UserDb};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::session::SessionDb;
use quench_core::bytes::Bytes;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::request::Request;
use std::sync::Arc;
use tokio::sync::MutexGuard;

type Container = Arc<quench_http::di::Container>;

fn catalog() -> PermissionCatalog {
    let dir = std::env::temp_dir().join(format!(
        "pages-api-test-{}",
        quench_core::uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(
        &path,
        r#"
[services.gatehouse]
actions = ["read-users", "create-user", "edit-user", "delete-user", "manage-permissions"]

[services.pagesdemo]
label = "Pages demo"
home = true
description = "A card for the tests."
actions = ["read", "write"]

[services.hidden]
label = "Hidden"
home = true
actions = ["read"]

[templates.viewer]
pagesdemo = ["read"]
"#,
    )
    .unwrap();
    let result = PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

struct World {
    app: Arc<dyn Endpoint>,
    container: Container,
    config: JwtConfig,
    db: Db,
    tokens: Arc<VerificationTokens>,
    mailer: Arc<RecordingSender>,
}

async fn world() -> World {
    quench_core::env::set("GATEHOUSE_KEY_ENCRYPTION_KEY", TEST_KEY_MATERIAL);
    gatehouse_service::api::pages::register_routes();
    let db = Db::connect("").await.expect("in-memory db");
    let config = JwtConfig::for_tests_with_signing();
    let tokens = Arc::new(VerificationTokens::in_memory());
    let mailer = Arc::new(RecordingSender::default());
    let sender: Arc<dyn Sender> = mailer.clone();
    let container = ContainerBuilder::new()
        .provide(config.clone())
        .provide(db.clone())
        .provide(catalog())
        .provide(sender)
        .provide(PublicBase::resolve("https://mail.example.test", ""))
        .provide_arc(tokens.clone())
        .provide_arc(SessionDb::init(quench_cache::CacheStore::in_memory()))
        .provide_arc(UserDb::init(db.clone()).await)
        .build()
        .await
        .unwrap();
    World {
        app: quench_starter::http::discover_and_mount("/"),
        container: Arc::new(container),
        config,
        db,
        tokens,
        mailer,
    }
}

async fn auth_enabled() -> MutexGuard<'static, ()> {
    let guard = service_auth_env_lock().lock().await;
    quench_core::env::set("SERVICE_AUTH_ENABLED", "true");
    guard
}

async fn auth_disabled() -> MutexGuard<'static, ()> {
    let guard = service_auth_env_lock().lock().await;
    quench_core::env::set("SERVICE_AUTH_ENABLED", "false");
    guard
}

async fn person(world: &World, username: &str, role: Role) -> User {
    realm::create(
        &world.db,
        &catalog(),
        true,
        username,
        "password",
        vec![role],
        Permissions::new(),
        Some(format!("{username}@example.test")),
    )
    .await
    .expect("seed user")
}

/// A bearer token for `sub`, holding `scope` (`gatehouse:read-users gatehouse:edit-user`).
async fn bearer(world: &World, sub: &str, scope: &str) -> String {
    let claims = Claims::for_audiences(
        sub.to_string(),
        vec![world.config.service_name.clone()],
        scope.to_string(),
        None,
        3600,
    );
    format!(
        "Bearer {}",
        world.config.encode_claims(&claims).await.unwrap()
    )
}

fn request(
    world: &World,
    method: Method,
    path: &str,
    token: Option<&str>,
    body: Option<&quench_serde::Value>,
) -> Request {
    let mut headers = HeaderMap::new();
    if let Some(token) = token {
        headers.insert("authorization", token.parse().unwrap());
    }
    let bytes = body.map_or_else(Bytes::new, |b| Bytes::from(quench_serde::json::to_vec(b)));
    if body.is_some() {
        headers.insert("content-type", "application/json".parse().unwrap());
    }
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(bytes),
        world.container.clone(),
    )
}

async fn call(
    world: &World,
    method: Method,
    path: &str,
    token: Option<&str>,
    body: Option<&quench_serde::Value>,
) -> (StatusCode, String) {
    let response = world
        .app
        .call(request(world, method, path, token, body))
        .await
        .into_inner();
    let status = response.status;
    let bytes = response.body.collect_limited(usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn read<T: quench_serde::DeserializeOwned>(
    world: &World,
    path: &str,
    token: Option<&str>,
) -> T {
    let (status, body) = call(world, Method::GET, path, token, None).await;
    assert_eq!(status, StatusCode::OK, "{path}: {body}");
    quench_serde::json::from_str(&body).unwrap_or_else(|e| panic!("{path}: {e}: {body}"))
}

#[quench_test::test]
async fn nothing_is_served_without_a_session() {
    let _guard = auth_enabled().await;
    let world = world().await;
    for path in [
        "/api/v1/ui/dashboard",
        "/api/v1/ui/account",
        "/api/v1/ui/mfa-enroll",
        "/api/v1/ui/admin/users",
        "/api/v1/ui/admin/users/alice",
    ] {
        let (status, _) = call(&world, Method::GET, path, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[quench_test::test]
async fn the_dashboard_has_a_card_for_each_service_that_is_switched_on() {
    let _guard = auth_enabled().await;
    quench_core::env::set("PAGESDEMO_UI_URL", "https://demo.example.test/pagesdemo/");
    // `hidden` has no URL, so it has no card.
    let world = world().await;
    person(&world, "alice", Role::User).await;
    let token = bearer(&world, "alice", "pagesdemo:read").await;

    let dashboard: Dashboard = read(&world, "/api/v1/ui/dashboard", Some(&token)).await;
    assert!(!dashboard.admin);
    let card = dashboard
        .services
        .iter()
        .find(|c| c.label == "Pages demo")
        .expect("the card");
    assert_eq!(card.url, "https://demo.example.test/pagesdemo");
    assert_eq!(card.title_key, "ui_service_pagesdemo_title");
    assert_eq!(card.desc_key, "ui_service_pagesdemo_desc");
    assert_eq!(card.description.as_deref(), Some("A card for the tests."));
    assert_eq!(card.card_class, "home-card-pagesdemo");
    assert!(dashboard.services.iter().all(|c| c.label != "Hidden"));

    let admin = bearer(&world, "root", "admin").await;
    let dashboard: Dashboard = read(&world, "/api/v1/ui/dashboard", Some(&admin)).await;
    assert!(dashboard.admin);
}

#[quench_test::test]
async fn the_dashboard_offers_only_the_services_the_person_may_use() {
    let _guard = auth_enabled().await;
    quench_core::env::set("PAGESDEMO_UI_URL", "https://demo.example.test/pagesdemo/");
    quench_core::env::set("HIDDEN_UI_URL", "https://demo.example.test/hidden/");
    let world = world().await;
    person(&world, "alice", Role::User).await;
    let labels = |dashboard: &Dashboard| {
        dashboard
            .services
            .iter()
            .map(|c| c.label.clone())
            .collect::<Vec<_>>()
    };

    // No grant on any service: no card.
    let nothing = bearer(&world, "alice", "gatehouse:read-users").await;
    let dashboard: Dashboard = read(&world, "/api/v1/ui/dashboard", Some(&nothing)).await;
    assert!(
        !labels(&dashboard).contains(&"Pages demo".to_string()),
        "{:?}",
        labels(&dashboard)
    );
    assert!(!labels(&dashboard).contains(&"Hidden".to_string()));

    // A read grant on one: that card only.
    let one = bearer(&world, "alice", "pagesdemo:read").await;
    let dashboard: Dashboard = read(&world, "/api/v1/ui/dashboard", Some(&one)).await;
    assert!(labels(&dashboard).contains(&"Pages demo".to_string()));
    assert!(!labels(&dashboard).contains(&"Hidden".to_string()));

    // A grant scoped to a part of the service still gets its card in.
    let scoped = bearer(&world, "alice", "hidden:project:p1:read").await;
    let dashboard: Dashboard = read(&world, "/api/v1/ui/dashboard", Some(&scoped)).await;
    assert!(labels(&dashboard).contains(&"Hidden".to_string()));
    assert!(!labels(&dashboard).contains(&"Pages demo".to_string()));

    // A role that grants everything sees them all.
    let admin = bearer(&world, "root", "admin").await;
    let dashboard: Dashboard = read(&world, "/api/v1/ui/dashboard", Some(&admin)).await;
    assert!(labels(&dashboard).contains(&"Pages demo".to_string()));
    assert!(labels(&dashboard).contains(&"Hidden".to_string()));
}

#[quench_test::test]
async fn the_account_has_the_profile_the_choices_and_the_lists() {
    let _guard = auth_enabled().await;
    let world = world().await;
    person(&world, "alice", Role::User).await;
    let token = bearer(&world, "alice", "user").await;

    let account: Account = read(&world, "/api/v1/ui/account", Some(&token)).await;
    assert_eq!(account.username, "alice");
    assert_eq!(account.email.as_deref(), Some("alice@example.test"));
    assert!(!account.email_verified && !account.has_notice_address);
    assert!(!account.has_avatar && account.avatar_url.is_none());
    assert!(!account.mfa_enabled);
    assert_eq!(
        account.locales,
        ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"]
    );
    assert!(account.timezones.iter().any(|z| z == "Europe/Warsaw"));
    assert!(!account.subscriptions.is_empty());
    let first = &account.subscriptions[0];
    assert!(first.label_key.starts_with("ui_notification_"));
    assert!(!first.service.is_empty());
}

#[quench_test::test]
async fn an_uploaded_picture_is_there_but_never_sent_back_as_text() {
    let _guard = auth_enabled().await;
    let world = world().await;
    person(&world, "alice", Role::User).await;
    let token = bearer(&world, "alice", "user").await;
    let update = |avatar: &str| realm::UserChanges {
        avatar_url: Some(avatar.to_string()),
        ..realm::UserChanges::default()
    };
    let sessions = SessionDb::init(quench_cache::CacheStore::in_memory());

    realm::update(
        &world.db,
        &catalog(),
        &sessions,
        "alice",
        false,
        "alice",
        update("data:image/png;base64,AAAA"),
    )
    .await
    .unwrap();
    let account: Account = read(&world, "/api/v1/ui/account", Some(&token)).await;
    assert!(account.has_avatar);
    assert_eq!(
        account.avatar_url, None,
        "the bytes of an upload are not a text box's value"
    );

    realm::update(
        &world.db,
        &catalog(),
        &sessions,
        "alice",
        false,
        "alice",
        update("https://pics.example.test/me.png"),
    )
    .await
    .unwrap();
    let account: Account = read(&world, "/api/v1/ui/account", Some(&token)).await;
    assert!(account.has_avatar);
    assert_eq!(
        account.avatar_url.as_deref(),
        Some("https://pics.example.test/me.png")
    );
}

#[quench_test::test]
async fn with_sign_in_switched_off_the_pages_see_the_stand_in_administrator() {
    let _guard = auth_disabled().await;
    let world = world().await;
    person(&world, "admin", Role::Admin).await;

    let account: Account = read(&world, "/api/v1/ui/account", None).await;
    assert_eq!(account.username, "admin");
    let users: AdminUsers = read(&world, "/api/v1/ui/admin/users", None).await;
    assert_eq!(users.actor, "admin");
    assert!(users.admin && users.can_create);
}

#[quench_test::test]
async fn enrolling_an_authenticator_checks_the_code_against_the_secret_shown() {
    let _guard = auth_enabled().await;
    let world = world().await;
    person(&world, "alice", Role::User).await;
    let token = bearer(&world, "alice", "user").await;

    let enrollment: MfaEnrollment = read(&world, "/api/v1/ui/mfa-enroll", Some(&token)).await;
    assert!(enrollment.uri.starts_with("otpauth://totp/"));
    assert!(enrollment.uri.contains(&enrollment.secret));
    assert!(
        enrollment
            .qr
            .as_deref()
            .is_some_and(|q| q.starts_with("data:image/"))
    );

    // A wrong code is the person's to fix: 400, with the translation key, and nothing turned on.
    let (status, body) = call(
        &world,
        Method::POST,
        "/api/v1/ui/mfa-enroll",
        Some(&token),
        Some(&quench_serde::json!({ "secret": enrollment.secret, "code": "000000" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("ui_admin_error_mfa_code_invalid"), "{body}");
    assert!(!realm::get(&world.db, "alice").await.unwrap().mfa_enabled);

    let code = quench_crypto::totp::Totp::from_base32(&enrollment.secret)
        .unwrap()
        .code_now();
    let (status, body) = call(
        &world,
        Method::POST,
        "/api/v1/ui/mfa-enroll",
        Some(&token),
        Some(&quench_serde::json!({ "secret": enrollment.secret, "code": code })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(realm::get(&world.db, "alice").await.unwrap().mfa_enabled);
}

#[quench_test::test]
async fn the_user_list_needs_the_read_action_and_says_what_the_viewer_may_do() {
    let _guard = auth_enabled().await;
    let world = world().await;
    person(&world, "root", Role::Admin).await;
    person(&world, "alice", Role::User).await;

    let nobody = bearer(&world, "alice", "user").await;
    let (status, body) = call(
        &world,
        Method::GET,
        "/api/v1/ui/admin/users",
        Some(&nobody),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, _) = call(
        &world,
        Method::GET,
        "/api/v1/ui/admin/users/root",
        Some(&nobody),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let reader = bearer(&world, "alice", "gatehouse:read-users").await;
    let users: AdminUsers = read(&world, "/api/v1/ui/admin/users", Some(&reader)).await;
    assert_eq!(users.actor, "alice");
    assert!(!users.can_create && !users.admin);
    let root = users.users.iter().find(|u| u.username == "root").unwrap();
    assert!(root.wildcard);
    assert_eq!(root.roles, ["admin"]);

    let creator = bearer(
        &world,
        "alice",
        "gatehouse:read-users gatehouse:create-user",
    )
    .await;
    let users: AdminUsers = read(&world, "/api/v1/ui/admin/users", Some(&creator)).await;
    assert!(users.can_create);
}

#[quench_test::test]
async fn one_user_comes_with_the_catalog_and_the_viewers_powers() {
    let _guard = auth_enabled().await;
    let world = world().await;
    person(&world, "root", Role::Admin).await;
    person(&world, "alice", Role::User).await;
    let reader = bearer(&world, "alice", "gatehouse:read-users").await;

    let user: AdminUser = read(&world, "/api/v1/ui/admin/users/root", Some(&reader)).await;
    assert_eq!(
        (user.username.as_str(), user.role.as_str()),
        ("root", "admin")
    );
    assert!(user.wildcard && !user.is_self);
    assert!(
        !user.can_edit && !user.can_delete && !user.can_manage_permissions && !user.viewer_admin
    );
    assert!(!user.disabled && !user.locked && !user.mfa_enabled);
    assert!(user.created_at.ends_with(" UTC"), "{}", user.created_at);
    assert_eq!(user.last_login_at, None);
    let demo = user
        .services
        .iter()
        .find(|s| s.name == "pagesdemo")
        .unwrap();
    assert_eq!(
        (demo.label.as_str(), demo.actions.as_slice()),
        (
            "Pages demo",
            ["read".to_string(), "write".to_string()].as_slice()
        )
    );
    assert_eq!(user.templates, ["viewer"]);

    let all = bearer(
        &world,
        "alice",
        "gatehouse:read-users gatehouse:edit-user gatehouse:delete-user gatehouse:manage-permissions",
    )
    .await;
    let own: AdminUser = read(&world, "/api/v1/ui/admin/users/alice", Some(&all)).await;
    assert!(own.is_self && own.can_edit && own.can_delete && own.can_manage_permissions);
    assert_eq!(own.email.as_deref(), Some("alice@example.test"));
    assert!(!own.email_verified);

    let (status, _) = call(
        &world,
        Method::GET,
        "/api/v1/ui/admin/users/ghost",
        Some(&all),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn the_grants_a_user_holds_are_listed_by_service() {
    let _guard = auth_enabled().await;
    let world = world().await;
    let sessions = SessionDb::init(quench_cache::CacheStore::in_memory());
    person(&world, "alice", Role::User).await;
    realm::apply_template(&world.db, &catalog(), &sessions, "root", "alice", "viewer")
        .await
        .unwrap();
    let reader = bearer(&world, "bob", "gatehouse:read-users").await;

    let user: AdminUser = read(&world, "/api/v1/ui/admin/users/alice", Some(&reader)).await;
    assert_eq!(user.held.len(), 1);
    assert_eq!(
        (
            user.held[0].service.as_str(),
            user.held[0].actions.as_slice()
        ),
        ("pagesdemo", ["read".to_string()].as_slice())
    );
    let users: AdminUsers = read(&world, "/api/v1/ui/admin/users", Some(&reader)).await;
    let row = users.users.iter().find(|u| u.username == "alice").unwrap();
    assert_eq!(row.grants, user.held);
}

#[quench_test::test]
async fn the_link_for_a_new_address_says_the_address_and_needs_no_session() {
    let _guard = auth_enabled().await;
    let world = world().await;
    let ticket = gatehouse_service::email_change::Ticket {
        username: "alice".into(),
        new_email: "new@example.test".into(),
    };
    let token = world
        .tokens
        .issue(PURPOSE_EMAIL_CHANGE, &ticket.encode(), 60)
        .await
        .unwrap();

    let found: ConfirmEmail = read(
        &world,
        &format!("/api/v1/ui/confirm-email?token={token}"),
        None,
    )
    .await;
    assert_eq!(found.new_email, "new@example.test");
    // Looking is not doing: the link still works.
    assert!(
        world
            .tokens
            .peek(PURPOSE_EMAIL_CHANGE, &token)
            .await
            .unwrap()
            .is_some()
    );

    let (status, _) = call(
        &world,
        Method::GET,
        "/api/v1/ui/confirm-email?token=nonsense",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(world.mailer.sent().is_empty());
}

#[quench_test::test]
async fn the_link_of_a_notification_names_the_kind_and_needs_no_session() {
    let _guard = auth_enabled().await;
    let world = world().await;
    let base = PublicBase::resolve("https://mail.example.test", "");
    let link = issue_link(&world.tokens, &base, "alice", "conveyor.run.failed")
        .await
        .unwrap();
    let token = link.split("token=").nth(1).unwrap();

    let info: UnsubscribeInfo = read(
        &world,
        &format!("/api/v1/ui/unsubscribe?token={token}"),
        None,
    )
    .await;
    assert_eq!(info.label_key, "ui_notification_conveyor_run_failed");
    for path in [
        "/api/v1/ui/unsubscribe?token=nonsense",
        "/api/v1/ui/unsubscribe",
    ] {
        let (status, _) = call(&world, Method::GET, path, None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[quench_test::test]
async fn the_notices_a_redirect_carries_are_the_keys_the_realm_speaks() {
    // The client shows only keys from its own lists; each must be a key the service can really send.
    use gatehouse_core::notice::{ACCOUNT_ERRORS, ADMIN_ERRORS, REGISTER_ERRORS};
    use gatehouse_service::realm::RealmError;
    let realm_keys = [
        RealmError::NotFound,
        RealmError::UsernameEmpty,
        RealmError::PasswordEmpty,
        RealmError::AlreadyExists,
        RealmError::UnknownGrants(Vec::new()),
        RealmError::LastAdmin,
        RealmError::SelfDemote,
        RealmError::SelfDelete,
        RealmError::SelfDisable,
        RealmError::UnknownTemplate,
        RealmError::RolesRequireAdmin,
        RealmError::MfaCodeInvalid,
        RealmError::CurrentPasswordInvalid,
        RealmError::UsernameInvalid,
        RealmError::EmailInvalid,
        RealmError::InviteNeedsEmail,
        RealmError::AlreadyConfirmed,
        RealmError::Internal,
    ]
    .map(|e| e.i18n_key());
    for key in ADMIN_ERRORS {
        assert!(realm_keys.contains(&key), "{key}");
    }
    for key in REGISTER_ERRORS
        .iter()
        .filter(|k| k.starts_with("ui_admin_"))
    {
        assert!(realm_keys.contains(key), "{key}");
    }
    for key in ACCOUNT_ERRORS
        .iter()
        .filter(|k| k.contains("_error_") && !k.starts_with("ui_account_error_"))
    {
        assert!(realm_keys.contains(key), "{key}");
    }
    // Every RealmError the admin pages can redirect with is one the client knows.
    for key in realm_keys {
        let known = ADMIN_ERRORS.contains(&key)
            || ACCOUNT_ERRORS.contains(&key)
            || REGISTER_ERRORS.contains(&key);
        assert!(known, "{key} would show nothing");
    }
    let _ = LoggingSender;
}
