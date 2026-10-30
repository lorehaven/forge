use gatehouse_service::PublicBase;
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::realm;
use gatehouse_service::test_support::{RecordingSender, auth_disabled_guard};
use gatehouse_service::tokens::VerificationTokens;
use gatehouse_service::ui::pages::admin::*;
use quench_auth::domain::auth::{Permissions, Role, User};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_core::bytes::Bytes;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::request::Request;
use std::collections::HashMap;
use std::sync::Arc;

async fn db() -> Db {
    Db::connect("").await.expect("in-memory db")
}

fn sessions() -> Arc<SessionDb> {
    SessionDb::init(quench_cache::CacheStore::in_memory())
}

async fn seed_user(db: &Db, username: &str, roles: Vec<Role>, grants: &[(&str, &[&str])]) -> User {
    let permissions: Permissions = grants
        .iter()
        .map(|(service, actions)| {
            (
                (*service).to_string(),
                actions
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
            )
        })
        .collect();
    realm::create(
        db,
        &catalog(),
        true,
        username,
        "password",
        roles,
        permissions,
        None,
    )
    .await
    .expect("seed user")
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

fn catalog() -> PermissionCatalog {
    let dir =
        std::env::temp_dir().join(format!("admin-test-{}", quench_core::uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(
        &path,
        r#"
        [services.conveyor]
        actions = ["read", "write"]
        resource_types = ["project"]

        [services.gatehouse]
        actions = ["read-users", "create-user", "edit-user", "delete-user", "manage-permissions"]
        "#,
    )
    .unwrap();
    let result = PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[quench_test::test]
fn a_resource_scoped_grant_survives_a_plain_checkbox_save() {
    let catalog = catalog();
    let mut existing = Permissions::new();
    existing.insert(
        "conveyor".to_string(),
        std::iter::once("project:abc-123:write".to_string()).collect(),
    );

    // The form checks conveyor's plain "read" box and leaves "write"
    // unchecked - as if an admin were narrowing the blanket grant, with
    // no idea the resource-scoped one even exists.
    let mut form = HashMap::new();
    form.insert("perm_conveyor_read".to_string(), "on".to_string());

    let result = permissions_from_form(&catalog, &form, &existing);
    let conveyor = result.get("conveyor").expect("conveyor grants survive");

    assert!(conveyor.contains("read"), "the checked box is honoured");
    assert!(
        !conveyor.contains("write"),
        "the unchecked plain box is dropped"
    );
    assert!(
        conveyor.contains("project:abc-123:write"),
        "the resource-scoped grant this form has no box for is preserved"
    );
}

#[quench_test::test]
fn a_plain_grant_can_still_be_revoked() {
    let catalog = catalog();
    let mut existing = Permissions::new();
    existing.insert(
        "conveyor".to_string(),
        std::iter::once("read".to_string()).collect(),
    );

    // Nothing checked at all - unchecking every box should still clear a
    // plain grant, not treat it as "unknown, so preserve it".
    let form = HashMap::new();

    let result = permissions_from_form(&catalog, &form, &existing);
    assert!(
        result
            .get("conveyor")
            .is_none_or(|actions| !actions.contains("read")),
        "an unchecked plain action is actually revoked"
    );
}

// -----------------------------------------------------------------
// parse_role
// -----------------------------------------------------------------

#[quench_test::test]
fn parse_role_accepts_known_roles_and_falls_back_to_user() {
    assert_eq!(parse_role(Some("admin")), Role::Admin);
    assert_eq!(parse_role(Some("service")), Role::Service);
    assert_eq!(parse_role(Some("user")), Role::User);
    assert_eq!(parse_role(Some("garbage")), Role::User);
    assert_eq!(parse_role(None), Role::User);
}

// -----------------------------------------------------------------
// HTTP handlers - every route here takes a private `admin_actor!`-generated
// extractor, so these go through the real discovered router.
// -----------------------------------------------------------------

async fn admin_app(
    config: JwtConfig,
    db: Db,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    admin_app_with_mailer(config, db, Arc::new(LoggingSender)).await
}

async fn admin_app_with_mailer(
    config: JwtConfig,
    db: Db,
    mailer: Arc<dyn Sender>,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::ui::pages::admin::register_routes();
    gatehouse_service::api::pages::register_routes();
    let container = ContainerBuilder::new()
        .provide(config)
        .provide(catalog())
        .provide(db)
        .provide(mailer)
        .provide(PublicBase::resolve("https://mail.example.test", ""))
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

fn post(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::POST,
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

// -----------------------------------------------------------------
// HTTP handlers - the "not signed in" guard branch
// -----------------------------------------------------------------

#[quench_test::test]
async fn users_page_redirects_to_login_when_not_signed_in() {
    let db = db().await;
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let (app, container) = admin_app(config, db).await;
    let resp = app.call(get("/ui/admin/users", &container)).await;
    assert!(resp.status().is_redirection());
}

// -----------------------------------------------------------------
// HTTP handlers - auth disabled (bypass claims, sub="admin")
// -----------------------------------------------------------------

#[quench_test::test]
async fn users_page_renders_for_the_bypass_admin() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app.call(get("/ui/admin/users", &container)).await;
    assert!(resp.status() == StatusCode::OK || resp.status() == StatusCode::NOT_FOUND);
    let resp = app.call(get("/api/v1/ui/admin/users", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[quench_test::test]
async fn users_page_slash_renders_for_the_bypass_admin() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app.call(get("/ui/admin/users/", &container)).await;
    assert!(resp.status() == StatusCode::OK || resp.status() == StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn edit_user_renders_a_known_user_and_404s_an_unknown_one() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;

    let resp = app
        .call(get("/api/v1/ui/admin/users/admin", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // The page is the client's, for any name; the API says there is no such user.
    let resp = app
        .call(get("/api/v1/ui/admin/users/no-such-user", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn create_user_creates_a_user_and_redirects_to_its_editor() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app
        .call(post_form(
            "/ui/admin/users",
            &[
                ("username", "brandnew"),
                ("password", "correct-horse"),
                ("role", "user"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = location(resp);
    assert!(location.contains("brandnew"));
    assert!(location.contains("ok=created"));
}

#[quench_test::test]
async fn create_user_reports_a_duplicate_username_via_the_list_redirect() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "brandnew", vec![Role::User], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app
        .call(post_form(
            "/ui/admin/users",
            &[
                ("username", "brandnew"),
                ("password", "correct-horse"),
                ("role", "user"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("/admin/users?err="));
}

#[quench_test::test]
async fn save_user_updates_permissions_via_the_checkbox_matrix() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app
        .call(post_form(
            "/ui/admin/users/target",
            &[("perm_conveyor_read", "on")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("ok=saved"));
}

#[quench_test::test]
async fn save_user_reports_not_found_for_an_unknown_target() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app
        .call(post_form("/ui/admin/users/no-such-user", &[], &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("/admin/users?err="));
}

#[quench_test::test]
async fn apply_template_reports_an_unknown_template() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app
        .call(post_form(
            "/ui/admin/users/target/template",
            &[("template", "no-such-template")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}

#[quench_test::test]
async fn disable_user_then_enable_user_round_trip() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;

    let resp = app
        .call(post("/ui/admin/users/target/disable", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("ok=saved"));

    let resp = app
        .call(post("/ui/admin/users/target/enable", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}

#[quench_test::test]
async fn disable_user_rejects_disabling_yourself() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app
        .call(post("/ui/admin/users/admin/disable", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("err="));
}

#[quench_test::test]
async fn unlock_user_clears_a_lockout() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    for _ in 0..5 {
        realm::authenticate(&db, "target", "wrong-password")
            .await
            .ok();
    }
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app
        .call(post("/ui/admin/users/target/unlock", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("ok=saved"));
}

#[quench_test::test]
async fn disable_user_mfa_turns_it_off() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app
        .call(post("/ui/admin/users/target/mfa/disable", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("ok=saved"));
}

#[quench_test::test]
async fn delete_user_removes_someone_else_but_not_yourself() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;

    let resp = app
        .call(post("/ui/admin/users/admin/delete", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let loc = location(resp);
    assert!(loc.contains("err="), "deleting yourself should fail: {loc}");

    let resp = app
        .call(post("/ui/admin/users/target/delete", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("ok=deleted"));
}

// -----------------------------------------------------------------
// Security notices when an administrator acts on someone's account
// -----------------------------------------------------------------

async fn seed_target(db: &Db, address: Option<&str>, confirmed: bool) {
    realm::create(
        db,
        &catalog(),
        true,
        "target",
        "password",
        vec![Role::User],
        Permissions::new(),
        address.map(str::to_string),
    )
    .await
    .expect("seed target");
    if confirmed {
        realm::mark_email_verified(db, "target")
            .await
            .expect("verify");
    }
}

fn admin_recording() -> (Arc<RecordingSender>, Arc<dyn Sender>) {
    let recorder = Arc::new(RecordingSender::default());
    let sender: Arc<dyn Sender> = recorder.clone();
    (recorder, sender)
}

#[quench_test::test]
async fn an_administrator_changing_a_password_tells_the_account_holder() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_target(&db, Some("target@example.test"), true).await;
    let (recorder, sender) = admin_recording();
    let (app, container) = admin_app_with_mailer(JwtConfig::for_tests(), db, sender).await;

    let resp = app
        .call(post_form(
            "/ui/admin/users/target",
            &[("password", "a-new-password-set-by-admin")],
            &container,
        ))
        .await;
    assert!(location(resp).contains("ok=saved"));
    let sent = recorder.sent_of("password-changed");
    assert_eq!(sent.len(), 1, "{:?}", recorder.sent());
    assert_eq!(sent[0].to, "target@example.test");
    assert_eq!(sent[0].username, "target");
}

#[quench_test::test]
async fn saving_other_changes_without_a_password_says_nothing() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_target(&db, Some("target@example.test"), true).await;
    let (recorder, sender) = admin_recording();
    let (app, container) = admin_app_with_mailer(JwtConfig::for_tests(), db, sender).await;
    let resp = app
        .call(post_form(
            "/ui/admin/users/target",
            &[("perm_conveyor_read", "on"), ("password", "")],
            &container,
        ))
        .await;
    assert!(location(resp).contains("ok=saved"));
    assert!(recorder.sent().is_empty(), "{:?}", recorder.sent());
}

#[quench_test::test]
async fn no_admin_notice_for_an_unconfirmed_or_missing_address() {
    for (address, confirmed) in [(Some("target@example.test"), false), (None, false)] {
        let _guard = auth_disabled_guard().await;
        let db = db().await;
        seed_user(&db, "admin", vec![Role::Admin], &[]).await;
        seed_target(&db, address, confirmed).await;
        let (recorder, sender) = admin_recording();
        let (app, container) = admin_app_with_mailer(JwtConfig::for_tests(), db, sender).await;
        app.call(post_form(
            "/ui/admin/users/target",
            &[("password", "a-new-password-set-by-admin")],
            &container,
        ))
        .await;
        assert!(recorder.sent().is_empty(), "{address:?}");
    }
}

#[quench_test::test]
async fn an_administrator_turning_off_mfa_tells_the_account_holder_only_if_it_was_on() {
    let _guard = auth_disabled_guard().await;
    quench_core::env::set(
        "GATEHOUSE_KEY_ENCRYPTION_KEY",
        gatehouse_service::test_support::TEST_KEY_MATERIAL,
    );
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_target(&db, Some("target@example.test"), true).await;
    let (recorder, sender) = admin_recording();
    let (app, container) = admin_app_with_mailer(JwtConfig::for_tests(), db.clone(), sender).await;

    // MFA never was on: no false alarm.
    app.call(post("/ui/admin/users/target/mfa/disable", &container))
        .await;
    assert!(recorder.sent().is_empty());

    // Turn it on, then have the administrator turn it off.
    let (secret, _) = realm::begin_mfa_enrollment("target").expect("enrollment");
    let code = quench_crypto::totp::Totp::from_base32(&secret)
        .expect("base32")
        .code_now();
    realm::enable_mfa(&db, "target", &secret, &code)
        .await
        .expect("enable mfa");
    app.call(post("/ui/admin/users/target/mfa/disable", &container))
        .await;
    let sent = recorder.sent_of("mfa-disabled");
    assert_eq!(sent.len(), 1, "{:?}", recorder.sent());
    assert_eq!(sent[0].to, "target@example.test");
}

// -----------------------------------------------------------------
// Inviting users
// -----------------------------------------------------------------

async fn admin_with_recorder() -> (
    Arc<RecordingSender>,
    Arc<dyn Endpoint>,
    Arc<quench_http::di::Container>,
    Db,
) {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let (recorder, sender) = admin_recording();
    let (app, container) = admin_app_with_mailer(JwtConfig::for_tests(), db.clone(), sender).await;
    (recorder, app, container, db)
}

#[quench_test::test]
async fn creating_a_user_with_an_address_sends_an_invitation() {
    let _guard = auth_disabled_guard().await;
    let (recorder, app, container, db) = admin_with_recorder().await;
    let resp = app
        .call(post_form(
            "/ui/admin/users",
            &[("username", "newbie"), ("email", "newbie@example.test")],
            &container,
        ))
        .await;
    let target = location(resp);
    assert!(
        target.contains("/admin/users/newbie?ok=invited"),
        "{target}"
    );

    let sent = recorder.sent_of("invite");
    assert_eq!(sent.len(), 1, "{:?}", recorder.sent());
    assert_eq!(sent[0].to, "newbie@example.test");
    assert!(sent[0].link.contains("/accept-invite?token="));
    let user = realm::get(&db, "newbie").await.unwrap();
    assert!(user.email_verified_at.is_none());
}

#[quench_test::test]
async fn a_password_typed_alongside_an_address_is_ignored() {
    let _guard = auth_disabled_guard().await;
    let (recorder, app, container, db) = admin_with_recorder().await;
    app.call(post_form(
        "/ui/admin/users",
        &[
            ("username", "newbie"),
            ("password", "admin-chose-this"),
            ("email", "newbie@example.test"),
        ],
        &container,
    ))
    .await;
    let user = realm::get(&db, "newbie").await.unwrap();
    assert!(
        !user.verify_password("admin-chose-this"),
        "the person picks their own password by accepting the invitation"
    );
    assert_eq!(recorder.sent_of("invite").len(), 1);
}

#[quench_test::test]
async fn creating_a_user_without_an_address_still_needs_a_password_and_sends_nothing() {
    let _guard = auth_disabled_guard().await;
    let (recorder, app, container, db) = admin_with_recorder().await;
    let resp = app
        .call(post_form(
            "/ui/admin/users",
            &[("username", "plain"), ("password", "given-password")],
            &container,
        ))
        .await;
    assert!(location(resp).contains("ok=created"));
    assert!(
        realm::get(&db, "plain")
            .await
            .unwrap()
            .verify_password("given-password")
    );
    assert!(recorder.sent().is_empty());

    let resp = app
        .call(post_form(
            "/ui/admin/users",
            &[("username", "nopw")],
            &container,
        ))
        .await;
    let target = location(resp);
    assert!(target.contains("/admin/users?err="), "{target}");
    assert!(realm::get(&db, "nopw").await.is_err());
}

#[quench_test::test]
async fn an_unusable_address_is_refused_and_creates_nothing() {
    let _guard = auth_disabled_guard().await;
    let (recorder, app, container, db) = admin_with_recorder().await;
    let resp = app
        .call(post_form(
            "/ui/admin/users",
            &[("username", "newbie"), ("email", "not-an-address")],
            &container,
        ))
        .await;
    assert!(location(resp).contains("err=ui_register_error_email_invalid"));
    assert!(realm::get(&db, "newbie").await.is_err());
    assert!(recorder.sent().is_empty());
}

#[quench_test::test]
async fn a_mail_failure_still_creates_the_account_and_says_so() {
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
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let (app, container) =
        admin_app_with_mailer(JwtConfig::for_tests(), db.clone(), Arc::new(Failing)).await;
    let resp = app
        .call(post_form(
            "/ui/admin/users",
            &[("username", "newbie"), ("email", "newbie@example.test")],
            &container,
        ))
        .await;
    assert!(location(resp).contains("ok=invite_failed"));
    assert!(
        realm::get(&db, "newbie").await.is_ok(),
        "the account exists"
    );
}

#[quench_test::test]
async fn resending_an_invitation_mails_the_unconfirmed_address_again() {
    let _guard = auth_disabled_guard().await;
    let (recorder, app, container, _db) = admin_with_recorder().await;
    app.call(post_form(
        "/ui/admin/users",
        &[("username", "newbie"), ("email", "newbie@example.test")],
        &container,
    ))
    .await;
    let resp = app
        .call(post("/ui/admin/users/newbie/invite", &container))
        .await;
    assert!(location(resp).contains("ok=invited"));
    let sent = recorder.sent_of("invite");
    assert_eq!(sent.len(), 2);
    assert_ne!(sent[0].link, sent[1].link, "a fresh token each time");
}

#[quench_test::test]
async fn resending_is_refused_for_a_confirmed_address_no_address_or_no_account() {
    let _guard = auth_disabled_guard().await;
    let (recorder, app, container, db) = admin_with_recorder().await;
    seed_target(&db, Some("t@example.test"), true).await;
    let resp = app
        .call(post("/ui/admin/users/target/invite", &container))
        .await;
    assert!(location(resp).contains("err=ui_admin_error_already_confirmed"));

    seed_user(&db, "bare", vec![Role::User], &[]).await;
    let resp = app
        .call(post("/ui/admin/users/bare/invite", &container))
        .await;
    assert!(location(resp).contains("err=ui_admin_error_invite_needs_email"));

    let resp = app
        .call(post("/ui/admin/users/ghost/invite", &container))
        .await;
    assert!(location(resp).contains("/admin/users?err="));
    assert!(recorder.sent().is_empty());
}
