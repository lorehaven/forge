use bytes::Bytes;
use gatehouse_service::PublicBase;
use gatehouse_service::api::users::*;
use gatehouse_service::catalog::PermissionCatalog;
use gatehouse_service::email::{LoggingSender, Sender};
use gatehouse_service::realm::{self, RealmError};
use gatehouse_service::test_support::{RecordingSender, service_auth_env_lock};
use gatehouse_service::tokens::VerificationTokens;
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
    app_with_mailer(db, catalog, jwt_config, Arc::new(LoggingSender)).await
}

async fn app_with_mailer(
    db: Db,
    catalog: PermissionCatalog,
    jwt_config: JwtConfig,
    mailer: Arc<dyn Sender>,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    gatehouse_service::api::users::register_routes();
    let container = ContainerBuilder::new()
        .provide(jwt_config)
        .provide(db.clone())
        .provide(catalog)
        .provide(mailer)
        .provide(PublicBase::resolve("https://mail.example.test", ""))
        .provide_arc(Arc::new(VerificationTokens::in_memory()))
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

#[tokio::test]
async fn a_password_change_through_the_api_tells_a_confirmed_address_only() {
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let recorder = Arc::new(RecordingSender::default());
    let sender: Arc<dyn Sender> = recorder.clone();
    let (app, container) = app_with_mailer(
        db.clone(),
        permission_catalog(),
        JwtConfig::for_tests(),
        sender,
    )
    .await;

    for (name, address) in [
        ("confirmed", "c@example.test"),
        ("unconfirmed", "u@example.test"),
    ] {
        app.call(json_req(
            Method::POST,
            "/api/v1/users",
            serde_json::json!({ "username": name, "password": "old-password", "email": address }),
            &container,
        ))
        .await;
    }
    realm::mark_email_verified(&db, "confirmed")
        .await
        .expect("verify");

    for name in ["confirmed", "unconfirmed"] {
        let resp = app
            .call(json_req(
                Method::PATCH,
                &format!("/api/v1/users/{name}"),
                serde_json::json!({ "password": "new-password" }),
                &container,
            ))
            .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
    // A PATCH that leaves the password alone says nothing.
    app.call(json_req(
        Method::PATCH,
        "/api/v1/users/confirmed",
        serde_json::json!({ "roles": ["user"] }),
        &container,
    ))
    .await;

    let sent = recorder.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(
        (sent[0].kind, sent[0].to.as_str()),
        ("password-changed", "c@example.test")
    );
}

async fn invite_app() -> (
    Arc<RecordingSender>,
    Arc<dyn Endpoint>,
    Arc<quench_http::di::Container>,
    Db,
) {
    let db = Db::connect("").await.expect("in-memory db");
    let recorder = Arc::new(RecordingSender::default());
    let sender: Arc<dyn Sender> = recorder.clone();
    let (app, container) = app_with_mailer(
        db.clone(),
        permission_catalog(),
        JwtConfig::for_tests(),
        sender,
    )
    .await;
    (recorder, app, container, db)
}

#[tokio::test]
async fn creating_an_invited_user_emails_the_address_and_reports_it() {
    let _guard = auth_disabled().await;
    let (recorder, app, container, db) = invite_app().await;
    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/users",
            serde_json::json!({ "username": "newbie", "email": "newbie@example.test", "invite": true }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: serde_json::Value = json_body(resp).await;
    assert_eq!(body["invite_sent"], true);
    assert_eq!(body["email"], "newbie@example.test");
    assert_eq!(body["email_verified"], false);

    let sent = recorder.sent_of("invite");
    assert_eq!(sent.len(), 1, "{:?}", recorder.sent());
    assert_eq!(sent[0].to, "newbie@example.test");
    let user = realm::get(&db, "newbie").await.unwrap();
    assert!(
        !user.verify_password(""),
        "no usable password until accepted"
    );
}

#[tokio::test]
async fn an_invitation_needs_a_valid_address() {
    let _guard = auth_disabled().await;
    let (recorder, app, container, db) = invite_app().await;
    for (body, name) in [
        (serde_json::json!({ "username": "a", "invite": true }), "a"),
        (
            serde_json::json!({ "username": "b", "invite": true, "email": "nonsense" }),
            "b",
        ),
    ] {
        let resp = app
            .call(json_req(Method::POST, "/api/v1/users", body, &container))
            .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{name}");
        assert!(
            realm::get(&db, name).await.is_err(),
            "{name} was not created"
        );
    }
    assert!(recorder.sent().is_empty());
}

#[tokio::test]
async fn an_ordinary_create_neither_invites_nor_mentions_it() {
    let _guard = auth_disabled().await;
    let (recorder, app, container, _db) = invite_app().await;
    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/users",
            serde_json::json!({ "username": "plain", "password": "given-password", "email": "p@example.test" }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: serde_json::Value = json_body(resp).await;
    assert!(body.get("invite_sent").is_none(), "{body}");
    assert!(recorder.sent().is_empty());
}

#[tokio::test]
async fn a_plain_create_still_needs_a_password() {
    let _guard = auth_disabled().await;
    let (_recorder, app, container, _db) = invite_app().await;
    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/users",
            serde_json::json!({ "username": "nopw" }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_failed_invitation_email_still_creates_the_account() {
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
    let _guard = auth_disabled().await;
    let db = Db::connect("").await.expect("in-memory db");
    let (app, container) = app_with_mailer(
        db.clone(),
        permission_catalog(),
        JwtConfig::for_tests(),
        Arc::new(Failing),
    )
    .await;
    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/users",
            serde_json::json!({ "username": "newbie", "email": "newbie@example.test", "invite": true }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: serde_json::Value = json_body(resp).await;
    assert_eq!(body["invite_sent"], false);
    assert!(realm::get(&db, "newbie").await.is_ok());
}
