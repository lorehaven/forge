use crate::support;

use actix_web::{App, test, web};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use quench_auth::prelude::UserDb;
use quench_auth::prelude::{Permissions, Role, User};
use quench_db::prelude::{Crud, Db};
use warehouse_service::docker_token::DockerTokenConfig;
use warehouse_service::routers::docker::token::{handle, validate_basic_encoded};

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

#[derive(serde::Deserialize)]
struct TokenResponseForTest {
    token: String,
    expires_in: usize,
}

#[actix_web::test]
async fn handle_issues_a_token_for_a_valid_basic_auth_credential() {
    let user_db = user_db_with("alice", "correct-horse").await;
    let config = config(true);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .app_data(web::Data::new(user_db))
            .service(handle),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/token?service=warehouse")
        .insert_header(("Authorization", basic_header("alice", "correct-horse")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

    let body: TokenResponseForTest = test::read_body_json(resp).await;
    assert_eq!(body.expires_in, 600);
    assert!(!body.token.is_empty());
}

#[actix_web::test]
async fn handle_rejects_a_wrong_password_with_unauthorized() {
    let user_db = user_db_with("alice", "correct-horse").await;
    let config = config(true);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .app_data(web::Data::new(user_db))
            .service(handle),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/token?service=warehouse")
        .insert_header(("Authorization", basic_header("alice", "wrong")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    assert!(resp.headers().contains_key("WWW-Authenticate"));
}

#[actix_web::test]
async fn handle_rejects_a_missing_authorization_header_when_auth_is_enabled() {
    let user_db = user_db_with("alice", "correct-horse").await;
    let config = config(true);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .app_data(web::Data::new(user_db))
            .service(handle),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/token?service=warehouse")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn handle_allows_anonymous_when_auth_is_disabled() {
    let user_db = user_db_with("alice", "correct-horse").await;
    let config = config(false);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .app_data(web::Data::new(user_db))
            .service(handle),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/token?service=warehouse")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn handle_rejects_a_service_name_mismatch() {
    let user_db = user_db_with("alice", "correct-horse").await;
    let config = config(false);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .app_data(web::Data::new(user_db))
            .service(handle),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/token?service=not-warehouse")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
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
