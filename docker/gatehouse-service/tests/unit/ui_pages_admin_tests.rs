use actix_web::body::to_bytes;
use actix_web::{App, HttpResponse, test as actix_test, web};
use gatehouse_service::api::auth::user_scope;
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::realm::{self, RealmError};
use gatehouse_service::ui::pages::admin::*;
use quench_auth::prelude::{Claims, JwtConfig, Permissions, Role, SessionDb, User};
use quench_db::prelude::Db;
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

async fn body_text(resp: HttpResponse) -> String {
    let body = to_bytes(resp.into_body()).await.expect("body");
    String::from_utf8(body.to_vec()).expect("utf8")
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
            ok: None,
        })
        .is_none()
    );
    assert!(
        notice_banner(&Notice {
            err: None,
            ok: Some("not-a-real-outcome".to_string()),
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
    assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    let html = body_text(resp).await;
    assert!(html.contains("ui_admin_forbidden"));
}

#[tokio::test]
async fn error_page_renders_with_the_error_s_own_status() {
    let resp = error_page(&RealmError::NotFound);
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
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

    let data = web::Data::new(db);
    let resp = render_list(&data, &claims, &Notice::default()).await;
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

    let data = web::Data::new(db);

    let admin_resp = render_list(&data, &claims_for(&admin), &Notice::default()).await;
    let admin_html = body_text(admin_resp).await;
    assert!(admin_html.contains("ui_admin_create_title"));
    assert!(
        admin_html.contains("ui_admin_you"),
        "admin sees themself tagged"
    );

    let plain_resp = render_list(&data, &claims_for(&plain), &Notice::default()).await;
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
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
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
// HTTP handlers - the "not signed in" guard branch
// -----------------------------------------------------------------

#[actix_web::test]
async fn users_page_redirects_to_login_when_not_signed_in() {
    let db = db().await;
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .app_data(web::Data::new(db))
            .service(users_page),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/admin/users")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

// -----------------------------------------------------------------
// HTTP handlers - auth disabled (bypass claims, sub="admin")
// -----------------------------------------------------------------

#[actix_web::test]
async fn users_page_renders_for_the_bypass_admin() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .service(users_page),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/admin/users")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn users_page_slash_renders_for_the_bypass_admin() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .service(users_page_slash),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/admin/users/")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn edit_user_renders_a_known_user_and_404s_an_unknown_one() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(catalog()))
            .app_data(web::Data::new(db))
            .service(edit_user),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/admin/users/admin")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

    let req = actix_test::TestRequest::get()
        .uri("/admin/users/no-such-user")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn create_user_creates_a_user_and_redirects_to_its_editor() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(catalog()))
            .app_data(web::Data::new(db))
            .service(create_user),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/admin/users")
        .set_form([
            ("username", "brandnew"),
            ("password", "correct-horse"),
            ("role", "user"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("brandnew"));
    assert!(location.contains("ok=created"));
}

#[actix_web::test]
async fn create_user_reports_a_duplicate_username_via_the_list_redirect() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "brandnew", vec![Role::User], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(catalog()))
            .app_data(web::Data::new(db))
            .service(create_user),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/admin/users")
        .set_form([
            ("username", "brandnew"),
            ("password", "correct-horse"),
            ("role", "user"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/admin/users?err="));
}

#[actix_web::test]
async fn save_user_updates_permissions_via_the_checkbox_matrix() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(catalog()))
            .app_data(web::Data::new(db))
            .app_data(web::Data::new(sessions()))
            .service(save_user),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/admin/users/target")
        .set_form([("perm_conveyor_read", "on")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("ok=saved"));
}

#[actix_web::test]
async fn save_user_reports_not_found_for_an_unknown_target() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(catalog()))
            .app_data(web::Data::new(db))
            .app_data(web::Data::new(sessions()))
            .service(save_user),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/admin/users/no-such-user")
        .set_form(Vec::<(&str, &str)>::new())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/admin/users?err="));
}

#[actix_web::test]
async fn apply_template_reports_an_unknown_template() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(catalog()))
            .app_data(web::Data::new(db))
            .app_data(web::Data::new(sessions()))
            .service(apply_template),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/admin/users/target/template")
        .set_form([("template", "no-such-template")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
}

#[actix_web::test]
async fn disable_user_then_enable_user_round_trip() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .service(disable_user)
            .service(enable_user),
    )
    .await;

    let req = actix_test::TestRequest::post()
        .uri("/admin/users/target/disable")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("ok=saved"));

    let req = actix_test::TestRequest::post()
        .uri("/admin/users/target/enable")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
}

#[actix_web::test]
async fn disable_user_rejects_disabling_yourself() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .service(disable_user),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/admin/users/admin/disable")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("err="));
}

#[actix_web::test]
async fn unlock_user_clears_a_lockout() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    for _ in 0..5 {
        realm::authenticate(&db, "target", "wrong-password")
            .await
            .ok();
    }
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .service(unlock_user),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/admin/users/target/unlock")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("ok=saved"));
}

#[actix_web::test]
async fn disable_user_mfa_turns_it_off() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .service(disable_user_mfa),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/admin/users/target/mfa/disable")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("ok=saved"));
}

#[actix_web::test]
async fn delete_user_removes_someone_else_but_not_yourself() {
    let db = db().await;
    seed_user(&db, "admin", vec![Role::Admin], &[]).await;
    seed_user(&db, "target", vec![Role::User], &[]).await;
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(db))
            .app_data(web::Data::new(sessions()))
            .service(delete_user),
    )
    .await;

    let req = actix_test::TestRequest::post()
        .uri("/admin/users/admin/delete")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        location.contains("err="),
        "deleting yourself should fail: {location}"
    );

    let req = actix_test::TestRequest::post()
        .uri("/admin/users/target/delete")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FOUND);
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("ok=deleted"));
}
