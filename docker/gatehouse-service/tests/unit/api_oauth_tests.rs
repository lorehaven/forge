use actix_web::{App, web};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use gatehouse_service::api::oauth::*;
use gatehouse_service::clients::{ClientRow, hash_secret};
use gatehouse_service::codes::AuthorizationCodeRow;
use quench_auth::prelude::{JwtConfig, Permissions, Role, SessionDb, User, UserDb};
use quench_cache::CacheStore;
use quench_db::prelude::{Crud, Db};
use sha2::{Digest, Sha256};

fn empty_token_request(grant_type: &str) -> TokenRequest {
    TokenRequest {
        grant_type: grant_type.to_string(),
        code: None,
        redirect_uri: None,
        code_verifier: None,
        client_id: None,
        client_secret: None,
        refresh_token: None,
    }
}

async fn seed_client(db: &Db, client_id: &str, secret: &str, redirect_uri: &str) -> ClientRow {
    let row = ClientRow {
        client_id: client_id.to_string(),
        secret_hash: hash_secret(secret),
        redirect_uris: vec![redirect_uri.to_string()],
        allowed_scopes: vec!["openid".to_string()],
        created_at: Utc::now(),
    };
    db.repository::<ClientRow>()
        .create(&row)
        .await
        .expect("seed client");
    row
}

async fn seed_user(db: &Db, username: &str) -> User {
    let user = User::new(
        username.to_string(),
        "password123".to_string(),
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .expect("build user");
    db.repository::<User>()
        .create(&user)
        .await
        .expect("seed user");
    user
}

fn pkce_pair() -> (String, String) {
    let verifier = "a-fixed-code-verifier-for-tests-1234567890";
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier.to_string(), challenge)
}

async fn seed_code(
    db: &Db,
    client_id: &str,
    username: &str,
    redirect_uri: &str,
    challenge: &str,
    usable: bool,
) -> String {
    let code = "a-fixed-authorization-code-for-tests";
    let now = Utc::now();
    let row = AuthorizationCodeRow {
        code_hash: hash_secret(code),
        client_id: client_id.to_string(),
        username: username.to_string(),
        redirect_uri: redirect_uri.to_string(),
        scope: "openid".to_string(),
        pkce_challenge: challenge.to_string(),
        created_at: now,
        expires_at: if usable {
            now + chrono::Duration::seconds(60)
        } else {
            now - chrono::Duration::seconds(1)
        },
        consumed_at: None,
    };
    db.repository::<AuthorizationCodeRow>()
        .create(&row)
        .await
        .expect("seed code");
    code.to_string()
}

#[test]
fn random_code_produces_distinct_url_safe_values() {
    let a = random_code();
    let b = random_code();
    assert_ne!(a, b);
    assert!(!a.contains('+') && !a.contains('/'));
}

#[tokio::test]
async fn authorization_code_grant_rejects_missing_fields() {
    let db = Db::connect("").await.expect("in-memory db");
    let config = JwtConfig::for_tests_with_signing();
    let users = UserDb::init(db.clone()).await;
    let sessions = SessionDb::init(CacheStore::in_memory());

    let resp = authorization_code_grant(
        &empty_token_request("authorization_code"),
        &config,
        &db,
        &users,
        &sessions,
    )
    .await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn authorization_code_grant_rejects_an_unknown_client() {
    let db = Db::connect("").await.expect("in-memory db");
    let config = JwtConfig::for_tests_with_signing();
    let users = UserDb::init(db.clone()).await;
    let sessions = SessionDb::init(CacheStore::in_memory());

    let body = TokenRequest {
        code: Some("whatever".to_string()),
        redirect_uri: Some("https://example.test/cb".to_string()),
        client_id: Some("no-such-client".to_string()),
        client_secret: Some("secret".to_string()),
        ..empty_token_request("authorization_code")
    };
    let resp = authorization_code_grant(&body, &config, &db, &users, &sessions).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn authorization_code_grant_rejects_the_wrong_client_secret() {
    let db = Db::connect("").await.expect("in-memory db");
    let config = JwtConfig::for_tests_with_signing();
    let users = UserDb::init(db.clone()).await;
    let sessions = SessionDb::init(CacheStore::in_memory());
    seed_client(&db, "client-a", "correct-secret", "https://example.test/cb").await;

    let body = TokenRequest {
        code: Some("whatever".to_string()),
        redirect_uri: Some("https://example.test/cb".to_string()),
        client_id: Some("client-a".to_string()),
        client_secret: Some("wrong-secret".to_string()),
        ..empty_token_request("authorization_code")
    };
    let resp = authorization_code_grant(&body, &config, &db, &users, &sessions).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn authorization_code_grant_rejects_an_unusable_code() {
    let db = Db::connect("").await.expect("in-memory db");
    let config = JwtConfig::for_tests_with_signing();
    let users = UserDb::init(db.clone()).await;
    let sessions = SessionDb::init(CacheStore::in_memory());
    seed_client(&db, "client-a", "correct-secret", "https://example.test/cb").await;
    seed_user(&db, "alice").await;
    let (verifier, challenge) = pkce_pair();
    let code = seed_code(
        &db,
        "client-a",
        "alice",
        "https://example.test/cb",
        &challenge,
        false,
    )
    .await;

    let body = TokenRequest {
        code: Some(code),
        redirect_uri: Some("https://example.test/cb".to_string()),
        client_id: Some("client-a".to_string()),
        client_secret: Some("correct-secret".to_string()),
        code_verifier: Some(verifier),
        ..empty_token_request("authorization_code")
    };
    let resp = authorization_code_grant(&body, &config, &db, &users, &sessions).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn authorization_code_grant_rejects_a_pkce_verifier_mismatch() {
    let db = Db::connect("").await.expect("in-memory db");
    let config = JwtConfig::for_tests_with_signing();
    let users = UserDb::init(db.clone()).await;
    let sessions = SessionDb::init(CacheStore::in_memory());
    seed_client(&db, "client-a", "correct-secret", "https://example.test/cb").await;
    seed_user(&db, "alice").await;
    let (_verifier, challenge) = pkce_pair();
    let code = seed_code(
        &db,
        "client-a",
        "alice",
        "https://example.test/cb",
        &challenge,
        true,
    )
    .await;

    let body = TokenRequest {
        code: Some(code),
        redirect_uri: Some("https://example.test/cb".to_string()),
        client_id: Some("client-a".to_string()),
        client_secret: Some("correct-secret".to_string()),
        code_verifier: Some("the-wrong-verifier".to_string()),
        ..empty_token_request("authorization_code")
    };
    let resp = authorization_code_grant(&body, &config, &db, &users, &sessions).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn authorization_code_grant_succeeds_and_the_code_cannot_be_replayed() {
    let db = Db::connect("").await.expect("in-memory db");
    let config = JwtConfig::for_tests_with_signing();
    let users = UserDb::init(db.clone()).await;
    let sessions = SessionDb::init(CacheStore::in_memory());
    seed_client(&db, "client-a", "correct-secret", "https://example.test/cb").await;
    seed_user(&db, "alice").await;
    let (verifier, challenge) = pkce_pair();
    let code = seed_code(
        &db,
        "client-a",
        "alice",
        "https://example.test/cb",
        &challenge,
        true,
    )
    .await;

    let body = TokenRequest {
        code: Some(code),
        redirect_uri: Some("https://example.test/cb".to_string()),
        client_id: Some("client-a".to_string()),
        client_secret: Some("correct-secret".to_string()),
        code_verifier: Some(verifier),
        ..empty_token_request("authorization_code")
    };
    let resp = authorization_code_grant(&body, &config, &db, &users, &sessions).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

    // Replaying the same code must fail - it was marked consumed above.
    let replay = authorization_code_grant(&body, &config, &db, &users, &sessions).await;
    assert_eq!(replay.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn refresh_token_grant_rejects_a_missing_refresh_token() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.expect("in-memory db");
    let users = UserDb::init(db.clone()).await;
    let sessions = SessionDb::init(CacheStore::in_memory());

    let resp = refresh_token_grant(
        &empty_token_request("refresh_token"),
        &config,
        &users,
        &sessions,
    )
    .await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn refresh_token_grant_rejects_an_unknown_token() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.expect("in-memory db");
    let users = UserDb::init(db.clone()).await;
    let sessions = SessionDb::init(CacheStore::in_memory());

    let body = TokenRequest {
        refresh_token: Some("not-a-real-refresh-token".to_string()),
        ..empty_token_request("refresh_token")
    };
    let resp = refresh_token_grant(&body, &config, &users, &sessions).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_token_grant_succeeds_for_a_live_session() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.expect("in-memory db");
    let users = UserDb::init(db.clone()).await;
    seed_user(&db, "alice").await;
    let sessions = SessionDb::init(CacheStore::in_memory());
    let (_, refresh_token) = sessions
        .create("alice", 3600)
        .await
        .expect("create session");

    let body = TokenRequest {
        refresh_token: Some(refresh_token),
        ..empty_token_request("refresh_token")
    };
    let resp = refresh_token_grant(&body, &config, &users, &sessions).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[tokio::test]
async fn client_credentials_grant_rejects_missing_fields() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.expect("in-memory db");

    let resp =
        client_credentials_grant(&empty_token_request("client_credentials"), &config, &db).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn client_credentials_grant_rejects_the_wrong_secret() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.expect("in-memory db");
    seed_client(&db, "machine-a", "correct-secret", "").await;

    let body = TokenRequest {
        client_id: Some("machine-a".to_string()),
        client_secret: Some("wrong-secret".to_string()),
        ..empty_token_request("client_credentials")
    };
    let resp = client_credentials_grant(&body, &config, &db).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn client_credentials_grant_succeeds_with_the_right_secret() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.expect("in-memory db");
    seed_client(&db, "machine-a", "correct-secret", "").await;

    let body = TokenRequest {
        client_id: Some("machine-a".to_string()),
        client_secret: Some("correct-secret".to_string()),
        ..empty_token_request("client_credentials")
    };
    let resp = client_credentials_grant(&body, &config, &db).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[tokio::test]
async fn subject_from_cookie_is_none_without_a_session_cookie() {
    let config = JwtConfig::for_tests_with_signing();
    let sessions = SessionDb::init(CacheStore::in_memory());
    let req = actix_web::test::TestRequest::default().to_http_request();
    assert!(
        subject_from_cookie(&req, &config, &sessions)
            .await
            .is_none()
    );
}

#[actix_web::test]
async fn token_endpoint_rejects_an_unsupported_grant_type() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.expect("in-memory db");
    let users = web::Data::new(UserDb::init(db.clone()).await);
    let sessions = web::Data::new(SessionDb::init(CacheStore::in_memory()));

    let app = actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .app_data(web::Data::new(db))
            .app_data(users)
            .app_data(sessions)
            .service(token),
    )
    .await;

    let req = actix_web::test::TestRequest::post()
        .uri("/api/v1/token")
        .set_form([("grant_type", "not-a-real-grant")])
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn authorize_endpoint_rejects_an_unknown_client() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.expect("in-memory db");
    let users = web::Data::new(UserDb::init(db.clone()).await);
    let sessions = web::Data::new(SessionDb::init(CacheStore::in_memory()));

    let app = actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .app_data(web::Data::new(db))
            .app_data(users)
            .app_data(sessions)
            .service(authorize),
    )
    .await;

    let req = actix_web::test::TestRequest::get()
        .uri(
            "/api/v1/authorize?client_id=no-such-client&redirect_uri=https://example.test/cb\
             &state=xyz&code_challenge=abc",
        )
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}
