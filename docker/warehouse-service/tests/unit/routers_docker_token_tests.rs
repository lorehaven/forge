use crate::support;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use http::{Method, StatusCode};
use quench_auth::domain::auth::{Permissions, Role, User, UserDb};
use quench_db::prelude::{Crud, Db};
use warehouse_service::docker_token::DockerTokenConfig;
use warehouse_service::routers::docker::token::{register_routes, validate_basic_encoded};

async fn user_db_with(username: &str, password: &str) -> std::sync::Arc<UserDb> {
    let db = Db::connect("").await.expect("in-memory database");
    let repo = db.repository::<User>();
    let user = User::new(
        username.to_string(),
        password.to_string(),
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .expect("build user");
    repo.create(&user).await.expect("seed user");
    UserDb::init(db).await
}

fn config(auth_enabled: bool) -> DockerTokenConfig {
    let _guard = support::secret_env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::set("DOCKER_TOKEN_SECRET", "token-handler-test-secret");
    DockerTokenConfig::init(
        "warehouse".to_string(),
        "https://warehouse.test/token".to_string(),
        auth_enabled,
    )
}

fn basic_header(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        STANDARD.encode(format!("{username}:{password}"))
    )
}

async fn app(
    config: DockerTokenConfig,
    user_db: std::sync::Arc<UserDb>,
) -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    register_routes();
    let container = support::container_builder()
        .provide(config)
        .provide_arc(user_db)
        .build()
        .await
        .unwrap();
    support::app(container).await
}

#[derive(serde::Deserialize)]
struct TokenResponseForTest {
    token: String,
    expires_in: usize,
}

#[tokio::test]
async fn handle_issues_a_token_for_a_valid_basic_auth_credential() {
    let user_db = user_db_with("alice", "correct-horse").await;
    let (app, container) = app(config(true), user_db).await;

    let req = support::raw_req(
        Method::GET,
        "/token?service=warehouse",
        &[("authorization", &basic_header("alice", "correct-horse"))],
        b"",
        &container,
    );
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = support::json_body(resp).await;
    let body: TokenResponseForTest = serde_json::from_value(body).unwrap();
    assert_eq!(body.expires_in, 600);
    assert!(!body.token.is_empty());
}

#[tokio::test]
async fn handle_rejects_a_wrong_password_with_unauthorized() {
    let user_db = user_db_with("alice", "correct-horse").await;
    let (app, container) = app(config(true), user_db).await;

    let req = support::raw_req(
        Method::GET,
        "/token?service=warehouse",
        &[("authorization", &basic_header("alice", "wrong"))],
        b"",
        &container,
    );
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let (headers, _) = support::parts(resp).await;
    assert!(headers.contains_key("www-authenticate"));
}

#[tokio::test]
async fn handle_rejects_a_missing_authorization_header_when_auth_is_enabled() {
    let user_db = user_db_with("alice", "correct-horse").await;
    let (app, container) = app(config(true), user_db).await;

    let req = support::req(Method::GET, "/token?service=warehouse", &container);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn handle_allows_anonymous_when_auth_is_disabled() {
    let user_db = user_db_with("alice", "correct-horse").await;
    let (app, container) = app(config(false), user_db).await;

    let req = support::req(Method::GET, "/token?service=warehouse", &container);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn handle_rejects_a_service_name_mismatch() {
    let user_db = user_db_with("alice", "correct-horse").await;
    let (app, container) = app(config(false), user_db).await;

    let req = support::req(Method::GET, "/token?service=not-warehouse", &container);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn validate_basic_encoded_rejects_malformed_base64_and_missing_colon() {
    let user_db = user_db_with("alice", "correct-horse").await;
    assert!(
        validate_basic_encoded("not-base64!!!", &user_db)
            .await
            .is_none()
    );
    assert!(
        validate_basic_encoded(&STANDARD.encode("no-colon-here"), &user_db)
            .await
            .is_none()
    );
}
