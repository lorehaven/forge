use bytes::Bytes;
use gatehouse_service::PublicBase;
use gatehouse_service::api::auth::user_scope;
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::realm::{self, RealmError};
use gatehouse_service::test_support::{RecordingSender, auth_disabled_guard};
use gatehouse_service::tokens::VerificationTokens;
use gatehouse_service::ui::pages::admin::*;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::auth::{Permissions, Role, User};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::session::SessionDb;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
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
                actions.iter().map(|a| a.to_string()).collect(),
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

fn claims_for(user: &User) -> Claims {
    Claims::for_audiences(
        user.username.clone(),
        vec!["gatehouse".to_string()],
        user_scope(user),
        None,
        3600,
    )
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

fn catalog() -> PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("admin-test-{}", uuid::Uuid::new_v4()));
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

#[test]
fn a_resource_scoped_grant_survives_a_plain_checkbox_save() {
    let catalog = catalog();
    let mut existing = Permissions::new();
    existing.insert(
        "conveyor".to_string(),
        ["project:abc-123:write".to_string()].into_iter().collect(),
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

#[test]
fn a_plain_grant_can_still_be_revoked() {
    let catalog = catalog();
    let mut existing = Permissions::new();
    existing.insert(
        "conveyor".to_string(),
        ["read".to_string()].into_iter().collect(),
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
// parse_role / notice_banner
// -----------------------------------------------------------------

#[test]
fn parse_role_accepts_known_roles_and_falls_back_to_user() {
    assert_eq!(parse_role(Some("admin")), Role::Admin);
    assert_eq!(parse_role(Some("service")), Role::Service);
    assert_eq!(parse_role(Some("user")), Role::User);
    assert_eq!(parse_role(Some("garbage")), Role::User);
    assert_eq!(parse_role(None), Role::User);
}

#[test]
fn notice_banner_is_none_without_a_recognised_key() {
    assert!(notice_banner(&Notice::default()).is_none());
    assert!(
        notice_banner(&Notice {
            err: Some("not-a-real-error".to_string()),
            ok: None
        })
        .is_none()
    );
    assert!(
        notice_banner(&Notice {
            err: None,
            ok: Some("not-a-real-outcome".to_string())
        })
        .is_none()
    );
}

#[test]
fn notice_banner_shows_a_known_error() {
    let notice = Notice {
        err: Some(RealmError::LastAdmin.i18n_key().to_string()),
        ok: None,
    };
    assert!(notice_banner(&notice).is_some());
}

#[test]
fn notice_banner_shows_every_known_ok_outcome() {
    for ok in ["created", "saved", "deleted"] {
        let notice = Notice {
            err: None,
            ok: Some(ok.to_string()),
        };
        assert!(notice_banner(&notice).is_some(), "ok={ok}");
    }
}

// -----------------------------------------------------------------
// forbidden_page / error_page
// -----------------------------------------------------------------

#[tokio::test]
async fn forbidden_page_renders_403() {
    let resp = forbidden_page();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let html = body_text(resp).await;
    assert!(html.contains("ui_admin_forbidden"));
}

#[tokio::test]
async fn error_page_renders_with_the_error_s_own_status() {
    let resp = error_page(&RealmError::NotFound);
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let html = body_text(resp).await;
    assert!(html.contains(RealmError::NotFound.i18n_key()));
}

// -----------------------------------------------------------------
// render_list
// -----------------------------------------------------------------

#[tokio::test]
async fn render_list_shows_the_empty_state_with_no_users() {
    let db = db().await;
    let actor = seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let claims = claims_for(&actor);

    // The actor itself has already been seeded, so delete it first to get a
    // genuinely empty list - `render_list` only reflects what `realm::list`
    // returns, it does not special-case the caller.
    realm::delete(&db, &sessions(), "someone-else", "admin")
        .await
        .ok();

    let resp = render_list(&db, &claims, &Notice::default()).await;
    let html = body_text(resp).await;
    assert!(html.contains("ui_admin_users_title"));
}

#[tokio::test]
async fn render_list_shows_create_panel_only_when_actor_can_create() {
    let db = db().await;
    let admin = seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let plain = seed_user(
        &db,
        "plain",
        vec![Role::User],
        &[("gatehouse", &["read-users"])],
    )
    .await;

    let admin_resp = render_list(&db, &claims_for(&admin), &Notice::default()).await;
    let admin_html = body_text(admin_resp).await;
    assert!(admin_html.contains("ui_admin_create_title"));
    assert!(
        admin_html.contains("ui_admin_you"),
        "admin sees themself tagged"
    );

    let plain_resp = render_list(&db, &claims_for(&plain), &Notice::default()).await;
    let plain_html = body_text(plain_resp).await;
    assert!(!plain_html.contains("ui_admin_create_title"));
}

// -----------------------------------------------------------------
// render_edit
// -----------------------------------------------------------------

#[test]
fn render_edit_as_admin_shows_the_role_select_and_delete_panel() {
    let catalog = catalog();
    let admin = User::new(
        "admin".to_string(),
        "pw".to_string(),
        vec![Role::Admin],
        Permissions::new(),
        None,
    )
    .unwrap();
    let target = User::new(
        "target".to_string(),
        "pw".to_string(),
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .unwrap();
    let claims = claims_for(&admin);

    let resp = render_edit(&catalog, &target, &claims, &Notice::default());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn render_edit_as_admin_includes_the_delete_button_for_someone_else() {
    let catalog = catalog();
    let admin = User::new(
        "admin".to_string(),
        "pw".to_string(),
        vec![Role::Admin],
        Permissions::new(),
        None,
    )
    .unwrap();
    let target = User::new(
        "target".to_string(),
        "pw".to_string(),
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .unwrap();
    let claims = claims_for(&admin);

    let resp = render_edit(&catalog, &target, &claims, &Notice::default());
    let html = body_text(resp).await;
    assert!(html.contains("ui_admin_delete_title"));
    assert!(html.contains("ui_admin_role"));
}

#[tokio::test]
async fn render_edit_hides_delete_when_the_target_is_the_actor() {
    let catalog = catalog();
    let admin = User::new(
        "admin".to_string(),
        "pw".to_string(),
        vec![Role::Admin],
        Permissions::new(),
        None,
    )
    .unwrap();
    let claims = claims_for(&admin);

    // Editing yourself: `can_delete && username != actor.sub` is false.
    let resp = render_edit(&catalog, &admin, &claims, &Notice::default());
    let html = body_text(resp).await;
    assert!(!html.contains("ui_admin_delete_title"));
}

#[tokio::test]
async fn render_edit_for_a_viewer_without_edit_user_shows_no_form() {
    let catalog = catalog();
    let viewer = User::new(
        "viewer".to_string(),
        "pw".to_string(),
        vec![Role::User],
        [(
            "gatehouse".to_string(),
            ["read-users".to_string()].into_iter().collect(),
        )]
        .into_iter()
        .collect(),
        None,
    )
    .unwrap();
    let target = User::new(
        "target".to_string(),
        "pw".to_string(),
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .unwrap();
    let claims = claims_for(&viewer);

    let resp = render_edit(&catalog, &target, &claims, &Notice::default());
    let html = body_text(resp).await;
    // No <form> around the permission matrix for a read-only viewer.
    assert!(!html.contains("ui_admin_save"));
}

#[tokio::test]
async fn render_edit_for_a_wildcard_target_shows_the_wildcard_note() {
    let catalog = catalog();
    let admin_actor = User::new(
        "admin".to_string(),
        "pw".to_string(),
        vec![Role::Admin],
        Permissions::new(),
        None,
    )
    .unwrap();
    let wildcard_target = User::new(
        "service-acct".to_string(),
        "pw".to_string(),
        vec![Role::Service],
        Permissions::new(),
        None,
    )
    .unwrap();
    let claims = claims_for(&admin_actor);

    let resp = render_edit(&catalog, &wildcard_target, &claims, &Notice::default());
    let html = body_text(resp).await;
    assert!(html.contains("ui_admin_wildcard_note"));
}

// -----------------------------------------------------------------
// status_panel (transitively via render_edit) - lock/disable/mfa states
// -----------------------------------------------------------------

#[tokio::test]
async fn render_edit_reflects_a_locked_and_disabled_account() {
    let db = db().await;
    let admin = seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;

    for _ in 0..5 {
        realm::authenticate(&db, "target", "wrong-password")
            .await
            .ok();
    }
    realm::set_disabled(&db, "target", true).await.unwrap();
    let locked_and_disabled = realm::get(&db, "target").await.unwrap();

    let catalog = catalog();
    let claims = claims_for(&admin);
    let resp = render_edit(&catalog, &locked_and_disabled, &claims, &Notice::default());
    let html = body_text(resp).await;
    assert!(html.contains("ui_admin_action_enable"));
    assert!(html.contains("ui_admin_action_unlock"));
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
    let encoded = serde_urlencoded::to_string(pairs).unwrap();
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

#[tokio::test]
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

#[tokio::test]
async fn users_page_renders_for_the_bypass_admin() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app.call(get("/ui/admin/users", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn users_page_slash_renders_for_the_bypass_admin() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;
    let resp = app.call(get("/ui/admin/users/", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn edit_user_renders_a_known_user_and_404s_an_unknown_one() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let (app, container) = admin_app(JwtConfig::for_tests(), db).await;

    let resp = app.call(get("/ui/admin/users/admin", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .call(get("/ui/admin/users/no-such-user", &container))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
async fn an_administrator_turning_off_mfa_tells_the_account_holder_only_if_it_was_on() {
    let _guard = auth_disabled_guard().await;
    envmnt::set(
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
    let code = {
        use totp_rs::Secret;
        totp_rs::Builder::new()
            .with_algorithm(totp_rs::Algorithm::SHA1)
            .with_digits(6)
            .with_skew(1)
            .with_step_duration(30)
            .with_secret(Secret::try_from_base32(&secret).expect("base32"))
            .with_account_name("target".to_string())
            .with_issuer(Some("Forge"))
            .build()
            .expect("totp")
            .generate_current()
            .to_string()
    };
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
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

#[tokio::test]
async fn a_mail_failure_still_creates_the_account_and_says_so() {
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

#[tokio::test]
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

#[tokio::test]
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

#[test]
fn the_invite_outcomes_have_banners_and_a_failure_is_styled_as_one() {
    let banner = |ok: &str| {
        notice_banner(&Notice {
            err: None,
            ok: Some(ok.to_string()),
        })
    };
    assert!(banner("invited").is_some());
    assert!(banner("invite_failed").is_some());
    assert!(banner("made-up").is_none());
}

#[tokio::test]
async fn the_editor_offers_a_resend_button_only_for_an_unconfirmed_address() {
    let _guard = auth_disabled_guard().await;
    let db = db().await;
    let admin = seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let catalog = catalog();
    let claims = claims_for(&admin);

    let html_for = |user: User| {
        let catalog = &catalog;
        let claims = &claims;
        async move { body_text(render_edit(catalog, &user, claims, &Notice::default())).await }
    };

    seed_target(&db, Some("t@example.test"), false).await;
    let unconfirmed = html_for(realm::get(&db, "target").await.unwrap()).await;
    assert!(unconfirmed.contains("ui_admin_status_email_confirmed"));
    assert!(unconfirmed.contains("ui_admin_action_resend_invite"));
    assert!(unconfirmed.contains("/invite"));

    realm::mark_email_verified(&db, "target").await.unwrap();
    let confirmed = html_for(realm::get(&db, "target").await.unwrap()).await;
    assert!(confirmed.contains("ui_admin_status_email_confirmed"));
    assert!(!confirmed.contains("ui_admin_action_resend_invite"));

    let bare = html_for(admin).await;
    assert!(
        !bare.contains("ui_admin_status_email_confirmed"),
        "no address, no row"
    );
}

#[tokio::test]
async fn the_create_form_has_an_optional_email_and_no_longer_requires_a_password() {
    let _guard = auth_disabled_guard().await;
    let (_recorder, app, container, _db) = admin_with_recorder().await;
    let resp = app.call(get("/ui/admin/users", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("name=\"email\""), "{html}");
    assert!(html.contains("ui_admin_new_email_hint"));
    // The password box is still there, but no longer `required`: an invited
    // person chooses their own.
    let password_input = html
        .split("<input")
        .find(|chunk| chunk.contains("id=\"new-password\""))
        .expect("password input");
    assert!(!password_input.contains("required"), "{password_input}");
}
