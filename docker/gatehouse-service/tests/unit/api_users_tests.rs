use bytes::Bytes;
use gatehouse_service::api::users::*;
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::realm::{self, RealmError};
use gatehouse_service::test_support::service_auth_env_lock;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::auth::{Role, UserDb};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::session::SessionDb;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
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

fn sessions() -> Arc<SessionDb> {
    SessionDb::init(quench_cache::CacheStore::in_memory())
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

async fn app(
    db: Db,
    catalog: PermissionCatalog,
    jwt_config: JwtConfig,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::api::users::register_routes();
    let container = ContainerBuilder::new()
        .provide(jwt_config)
        .provide(db.clone())
        .provide(catalog)
        .provide_arc(sessions())
        .provide_arc(UserDb::init(db).await)
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn req(method: Method, path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

fn req_with_auth(
    method: Method,
    path: &str,
    token: &str,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", token.parse().unwrap());
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

fn json_req(
    method: Method,
    path: &str,
    body: serde_json::Value,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
        container.clone(),
    )
}

async fn json_body<T: serde::de::DeserializeOwned>(resp: quench_http::response::Response) -> T {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    serde_json::from_slice(&collected.to_bytes()).expect("valid json body")
}

#[tokio::test]
async fn create_then_list_then_get_a_user() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let (app, container) = app(db, permission_catalog(), JwtConfig::for_tests()).await;

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/users",
            serde_json::json!({ "username": "alice", "password": "password123" }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created: UserView = json_body(resp).await;
    assert_eq!(created.username, "alice");
    assert_eq!(created.roles, vec![Role::User]);

    let resp = app
        .call(req(Method::GET, "/api/v1/users", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let users: Vec<UserView> = json_body(resp).await;
    assert!(users.iter().any(|u| u.username == "alice"));

    let resp = app
        .call(req(Method::GET, "/api/v1/users/alice", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_a_missing_user_reports_not_found_with_a_message() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let (app, container) = app(db, permission_catalog(), JwtConfig::for_tests()).await;

    let resp = app
        .call(req(Method::GET, "/api/v1/users/nobody", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let problem: Problem = json_body(resp).await;
    assert_eq!(problem.error, "no such user");
}

#[tokio::test]
async fn create_rejects_a_duplicate_username_with_conflict() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let (app, container) = app(db, permission_catalog(), JwtConfig::for_tests()).await;

    let body = serde_json::json!({ "username": "alice", "password": "password123" });
    app.call(json_req(
        Method::POST,
        "/api/v1/users",
        body.clone(),
        &container,
    ))
    .await;

    let resp = app
        .call(json_req(Method::POST, "/api/v1/users", body, &container))
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn update_changes_the_password() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let (app, container) = app(db.clone(), permission_catalog(), JwtConfig::for_tests()).await;

    app.call(json_req(
        Method::POST,
        "/api/v1/users",
        serde_json::json!({ "username": "alice", "password": "old-password" }),
        &container,
    ))
    .await;

    let resp = app
        .call(json_req(
            Method::PATCH,
            "/api/v1/users/alice",
            serde_json::json!({ "password": "new-password" }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let user = realm::get(&db, "alice").await.expect("get");
    assert!(user.verify_password("new-password"));
}

#[tokio::test]
async fn replace_permissions_rejects_unknown_grants() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let (app, container) = app(db, permission_catalog(), JwtConfig::for_tests()).await;

    app.call(json_req(
        Method::POST,
        "/api/v1/users",
        serde_json::json!({ "username": "alice", "password": "password123" }),
        &container,
    ))
    .await;

    let resp = app
        .call(json_req(
            Method::PUT,
            "/api/v1/users/alice/permissions",
            serde_json::json!({ "permissions": { "not-a-real-service": ["read"] } }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_removes_the_user() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let (app, container) = app(db.clone(), permission_catalog(), JwtConfig::for_tests()).await;

    app.call(json_req(
        Method::POST,
        "/api/v1/users",
        serde_json::json!({ "username": "alice", "password": "password123" }),
        &container,
    ))
    .await;

    let resp = app
        .call(req(Method::DELETE, "/api/v1/users/alice", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    assert!(matches!(
        realm::get(&db, "alice").await.unwrap_err(),
        RealmError::NotFound
    ));
}

#[tokio::test]
async fn me_reports_wildcard_access_for_the_anonymous_admin_bypass() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let (app, container) = app(db, permission_catalog(), JwtConfig::for_tests()).await;

    let resp = app.call(req(Method::GET, "/api/v1/me", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let effective_access: Me = json_body(resp).await;
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
    let (app, container) = app(
        db,
        permission_catalog(),
        JwtConfig::for_tests_with_signing(),
    )
    .await;

    let resp = app
        .call(req(Method::GET, "/api/v1/users", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_users_is_forbidden_without_the_read_users_action() {
    let _guard = auth_enabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    // The app and the token must share the same signing key, so build
    // the config once and register that exact instance as app data.
    let config = JwtConfig::for_tests_with_signing();
    let token = bearer(&config, "gatehouse:edit-user").await;
    let (app, container) = app(db, permission_catalog(), config).await;

    let resp = app
        .call(req_with_auth(
            Method::GET,
            "/api/v1/users",
            &token,
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_users_succeeds_with_the_read_users_action() {
    let _guard = auth_enabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let config = JwtConfig::for_tests_with_signing();
    let token = bearer(&config, "gatehouse:read-users").await;
    let (app, container) = app(db, permission_catalog(), config).await;

    let resp = app
        .call(req_with_auth(
            Method::GET,
            "/api/v1/users",
            &token,
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn list_users_succeeds_for_a_wildcard_admin_role() {
    let _guard = auth_enabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let config = JwtConfig::for_tests_with_signing();
    // `Claims::can` treats a wildcard role as satisfying any action on
    // any service - see the `action_claims!` doc comment.
    let token = bearer(&config, Role::Admin.as_str()).await;
    let (app, container) = app(db, permission_catalog(), config).await;

    let resp = app
        .call(req_with_auth(
            Method::GET,
            "/api/v1/users",
            &token,
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}
