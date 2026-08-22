use actix_web::body::to_bytes;
use actix_web::{App, test, web};
use gatehouse_service::api::users::*;
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::realm::{self, RealmError};
use gatehouse_service::test_support::service_auth_env_lock;
use quench_auth::prelude::{Claims, JwtConfig, Role, SessionDb, UserDb};
use quench_db::prelude::Db;
use std::sync::Arc;
use tokio::sync::MutexGuard;

fn permission_catalog() -> PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("gatehouse-users-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(
        &path,
        r#"
        [services.sage]
        actions = ["read", "write"]
        "#,
    )
    .unwrap();
    let result = PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

async fn sessions() -> web::Data<Arc<SessionDb>> {
    web::Data::new(SessionDb::init(quench_cache::CacheStore::in_memory()))
}

/// With `SERVICE_AUTH_ENABLED` off (this crate's dev/test bypass, and the
/// default `JwtConfig::for_tests()` reads), every `action_claims!`
/// extractor is satisfied by an anonymous admin identity - see
/// `SubjectClaims::from_request`. Holds the crate-wide lock for as long
/// as that assumption needs to hold, since `ui::tests` toggles this same
/// var to "true" for its own tests.
async fn auth_disabled() -> MutexGuard<'static, ()> {
    let guard = service_auth_env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };
    guard
}

/// `test::init_service`'s return type is a private, non-nameable `impl
/// Service`, so this builds the app inline at each call site rather than
/// through a helper function that would have to name it.
macro_rules! app_with {
    ($db:expr, $catalog:expr, $sessions:expr) => {
        app_with!($db, $catalog, $sessions, JwtConfig::for_tests())
    };
    ($db:expr, $catalog:expr, $sessions:expr, $jwt_config:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($jwt_config))
                .app_data(web::Data::new($db.clone()))
                .app_data(web::Data::new($catalog))
                .app_data($sessions)
                .service(scope()),
        )
        .await
    };
}

#[tokio::test]
async fn create_then_list_then_get_a_user() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let app = app_with!(db, permission_catalog(), sessions().await);

    let create_req = test::TestRequest::post()
        .uri("/api/v1/users")
        .set_json(serde_json::json!({
            "username": "alice",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, create_req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);
    let created: UserView = test::read_body_json(resp).await;
    assert_eq!(created.username, "alice");
    assert_eq!(created.roles, vec![Role::User]);

    let list_req = test::TestRequest::get().uri("/api/v1/users").to_request();
    let resp = test::call_service(&app, list_req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let users: Vec<UserView> = test::read_body_json(resp).await;
    assert!(users.iter().any(|u| u.username == "alice"));

    let get_req = test::TestRequest::get()
        .uri("/api/v1/users/alice")
        .to_request();
    let resp = test::call_service(&app, get_req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[tokio::test]
async fn get_a_missing_user_reports_not_found_with_a_message() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let app = app_with!(db, permission_catalog(), sessions().await);

    let req = test::TestRequest::get()
        .uri("/api/v1/users/nobody")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

    let body = to_bytes(resp.into_body()).await.expect("body");
    let problem: Problem = serde_json::from_slice(&body).expect("problem json");
    assert_eq!(problem.error, "no such user");
}

#[tokio::test]
async fn create_rejects_a_duplicate_username_with_conflict() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let app = app_with!(db, permission_catalog(), sessions().await);

    let body = serde_json::json!({ "username": "alice", "password": "password123" });
    let first = test::TestRequest::post()
        .uri("/api/v1/users")
        .set_json(&body)
        .to_request();
    test::call_service(&app, first).await;

    let second = test::TestRequest::post()
        .uri("/api/v1/users")
        .set_json(&body)
        .to_request();
    let resp = test::call_service(&app, second).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn update_changes_the_password() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let app = app_with!(db, permission_catalog(), sessions().await);

    let create = test::TestRequest::post()
        .uri("/api/v1/users")
        .set_json(serde_json::json!({ "username": "alice", "password": "old-password" }))
        .to_request();
    test::call_service(&app, create).await;

    let update = test::TestRequest::patch()
        .uri("/api/v1/users/alice")
        .set_json(serde_json::json!({ "password": "new-password" }))
        .to_request();
    let resp = test::call_service(&app, update).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

    let user = realm::get(&db, "alice").await.expect("get");
    assert!(user.verify_password("new-password"));
}

#[tokio::test]
async fn replace_permissions_rejects_unknown_grants() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let app = app_with!(db, permission_catalog(), sessions().await);

    let create = test::TestRequest::post()
        .uri("/api/v1/users")
        .set_json(serde_json::json!({ "username": "alice", "password": "password123" }))
        .to_request();
    test::call_service(&app, create).await;

    let replace = test::TestRequest::put()
        .uri("/api/v1/users/alice/permissions")
        .set_json(serde_json::json!({
            "permissions": { "not-a-real-service": ["read"] }
        }))
        .to_request();
    let resp = test::call_service(&app, replace).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_removes_the_user() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let app = app_with!(db, permission_catalog(), sessions().await);

    let create = test::TestRequest::post()
        .uri("/api/v1/users")
        .set_json(serde_json::json!({ "username": "alice", "password": "password123" }))
        .to_request();
    test::call_service(&app, create).await;

    let delete = test::TestRequest::delete()
        .uri("/api/v1/users/alice")
        .to_request();
    let resp = test::call_service(&app, delete).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);

    assert!(matches!(
        realm::get(&db, "alice").await.unwrap_err(),
        RealmError::NotFound
    ));
}

#[tokio::test]
async fn me_reports_wildcard_access_for_the_anonymous_admin_bypass() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    // `UserDb::init` already returns `Arc<UserDb>` - wrapping it in
    // another `Arc` here would silently mismatch the `web::Data<Arc<UserDb>>`
    // the `me` handler extracts, and fail with an unhelpful 500.
    let user_db = web::Data::new(UserDb::init(db.clone()).await);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .app_data(web::Data::new(permission_catalog()))
            .app_data(user_db)
            .service(me_scope()),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/v1/me").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

    let effective_access: Me = test::read_body_json(resp).await;
    assert!(effective_access.wildcard);
    assert_eq!(effective_access.username, "anonymous");
}

// -- with SERVICE_AUTH_ENABLED=true: the real `action_claims!` checks ---

async fn auth_enabled() -> MutexGuard<'static, ()> {
    let guard = service_auth_env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };
    guard
}

async fn bearer(config: &JwtConfig, scope: &str) -> String {
    let claims = Claims::for_audiences(
        "someone".to_string(),
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

#[tokio::test]
async fn list_users_is_unauthorized_without_a_token_when_auth_is_enabled() {
    let _guard = auth_enabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let app = app_with!(
        db,
        permission_catalog(),
        sessions().await,
        JwtConfig::for_tests_with_signing()
    );

    let req = test::TestRequest::get().uri("/api/v1/users").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_users_is_forbidden_without_the_read_users_action() {
    let _guard = auth_enabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    // The app and the token must share the same signing key, so build
    // the config once and register that exact instance as app data.
    let config = JwtConfig::for_tests_with_signing();
    let token = bearer(&config, "gatehouse:edit-user").await;
    let app = app_with!(db, permission_catalog(), sessions().await, config);

    let req = test::TestRequest::get()
        .uri("/api/v1/users")
        .insert_header(("Authorization", token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_users_succeeds_with_the_read_users_action() {
    let _guard = auth_enabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let config = JwtConfig::for_tests_with_signing();
    let token = bearer(&config, "gatehouse:read-users").await;
    let app = app_with!(db, permission_catalog(), sessions().await, config);

    let req = test::TestRequest::get()
        .uri("/api/v1/users")
        .insert_header(("Authorization", token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[tokio::test]
async fn list_users_succeeds_for_a_wildcard_admin_role() {
    let _guard = auth_enabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let config = JwtConfig::for_tests_with_signing();
    // `Claims::can` treats a wildcard role as satisfying any action on
    // any service - see the `action_claims!` doc comment.
    let token = bearer(&config, Role::Admin.as_str()).await;
    let app = app_with!(db, permission_catalog(), sessions().await, config);

    let req = test::TestRequest::get()
        .uri("/api/v1/users")
        .insert_header(("Authorization", token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}
