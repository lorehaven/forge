use gatehouse_service::api::app_tokens::*;
use gatehouse_service::api::oauth::TokenRequest;
use gatehouse_service::app_tokens::{AppTokenRow, TOKEN_PREFIX};
use gatehouse_service::clients::hash_secret;
use quench_auth::domain::auth::{Permissions, Role, User, UserDb};
use quench_auth::domain::jwt::JwtConfig;
use quench_core::time::{Duration, Time};
use quench_db::prelude::{Crud, Db};
use quench_http::http::StatusCode;

fn request(secret: Option<&str>) -> TokenRequest {
    TokenRequest {
        grant_type: "app_token".to_string(),
        code: None,
        redirect_uri: None,
        code_verifier: None,
        client_id: None,
        client_secret: None,
        refresh_token: None,
        app_token: secret.map(str::to_string),
    }
}

async fn seed_user(db: &Db, username: &str, grants: &[(&str, &str)]) -> User {
    let mut permissions = Permissions::new();
    for (service, action) in grants {
        permissions
            .entry((*service).to_string())
            .or_default()
            .insert((*action).to_string());
    }
    let user = User::new(
        username.to_string(),
        "password123".to_string(),
        vec![Role::User],
        permissions,
        None,
    )
    .expect("build user");
    db.repository::<User>().create(&user).await.expect("seed");
    user
}

fn body(name: &str, scope: &[&str]) -> CreateAppToken {
    CreateAppToken {
        name: name.to_string(),
        scope: scope.iter().map(|entry| (*entry).to_string()).collect(),
        expires_in_days: None,
    }
}

async fn store(db: &Db, user: &str, scope: &[&str]) -> (AppTokenRow, String) {
    let (row, secret) = new_row(user, &body("anvil", scope), Time::now());
    db.repository::<AppTokenRow>()
        .create(&row)
        .await
        .expect("store token");
    (row, secret)
}

async fn claims_of(
    resp: quench_http::response::Response,
    config: &JwtConfig,
) -> quench_auth::domain::jwt::Claims {
    let collected = resp
        .into_inner()
        .body
        .collect_limited(usize::MAX)
        .await
        .expect("body");
    let value: quench_serde::Value = quench_serde::json::from_slice(&collected).expect("json");
    let token = value["access_token"].as_str().expect("access_token");
    config.decode_claims(token).await.expect("decodes")
}

#[test]
fn new_row_stores_only_the_hash_of_a_prefixed_secret() {
    let (row, secret) = new_row(
        "alice",
        &body("  anvil ", &["warehouse:write"]),
        Time::now(),
    );
    assert!(secret.starts_with(TOKEN_PREFIX));
    assert_eq!(row.token_hash, hash_secret(&secret));
    assert_ne!(row.token_hash, secret);
    assert_eq!(row.name, "anvil");
    assert_eq!(row.entries(), vec!["warehouse:write".to_string()]);
    assert_eq!(row.id.len(), 12);
}

#[test]
fn new_row_caps_the_lifetime() {
    let now = Time::now();
    let mut request = body("anvil", &[]);
    request.expires_in_days = Some(10_000);
    let (row, _) = new_row("alice", &request, now);
    assert_eq!(row.expires_at, Some(now + Duration::days(365)));
}

#[tokio::test]
async fn refusal_checks_name_lifetime_and_scope_entries() {
    let db = Db::connect("").await.unwrap();
    let user = seed_user(&db, "alice", &[("warehouse", "write")]).await;

    assert!(refusal(&body("anvil", &["warehouse:write"]), &user).is_none());
    assert!(refusal(&body("anvil", &[]), &user).is_none());
    assert!(refusal(&body("  ", &[]), &user).is_some());
    assert!(refusal(&body("anvil", &["warehouse"]), &user).is_some());
    assert!(refusal(&body("anvil", &["conveyor:run"]), &user).is_some());
    let mut short = body("anvil", &[]);
    short.expires_in_days = Some(0);
    assert!(refusal(&short, &user).is_some());
}

#[tokio::test]
async fn grant_requires_the_secret() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.unwrap();
    let users = UserDb::init(db.clone()).await;

    let resp = app_token_grant(&request(None), &config, &db, &users).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn grant_rejects_an_unknown_secret() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.unwrap();
    let users = UserDb::init(db.clone()).await;

    let resp = app_token_grant(&request(Some("lht_nope")), &config, &db, &users).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn grant_rejects_revoked_and_expired_tokens() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.unwrap();
    let users = UserDb::init(db.clone()).await;
    seed_user(&db, "alice", &[("warehouse", "write")]).await;
    let tokens = db.repository::<AppTokenRow>();

    let (mut revoked, revoked_secret) = store(&db, "alice", &[]).await;
    revoked.revoked_at = Some(Time::now());
    tokens.update(&revoked).await.unwrap();
    let (mut expired, expired_secret) = store(&db, "alice", &[]).await;
    expired.expires_at = Some(Time::now() - Duration::days(1));
    tokens.update(&expired).await.unwrap();

    for secret in [revoked_secret, expired_secret] {
        let resp = app_token_grant(&request(Some(&secret)), &config, &db, &users).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn grant_rejects_a_token_whose_owner_is_gone() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.unwrap();
    let users = UserDb::init(db.clone()).await;
    let (_, secret) = store(&db, "ghost", &[]).await;

    let resp = app_token_grant(&request(Some(&secret)), &config, &db, &users).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn full_access_token_carries_the_owners_grants_and_records_use() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.unwrap();
    let users = UserDb::init(db.clone()).await;
    seed_user(
        &db,
        "alice",
        &[("warehouse", "write"), ("conveyor", "read")],
    )
    .await;
    let (row, secret) = store(&db, "alice", &[]).await;

    let resp = app_token_grant(&request(Some(&secret)), &config, &db, &users).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let claims = claims_of(resp, &config).await;
    assert_eq!(claims.sub, "alice");
    assert!(claims.can("warehouse", "write"));
    assert!(claims.can("conveyor", "read"));

    let stored = db
        .repository::<AppTokenRow>()
        .read(&row.token_hash)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.last_used_at.is_some());
}

#[tokio::test]
async fn narrowed_token_carries_only_its_entries() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.unwrap();
    let users = UserDb::init(db.clone()).await;
    seed_user(
        &db,
        "alice",
        &[("warehouse", "write"), ("conveyor", "read")],
    )
    .await;
    let (_, secret) = store(&db, "alice", &["warehouse:write"]).await;

    let resp = app_token_grant(&request(Some(&secret)), &config, &db, &users).await;
    let claims = claims_of(resp, &config).await;
    assert!(claims.can("warehouse", "write"));
    assert!(!claims.can("conveyor", "read"));
}

#[tokio::test]
async fn narrowed_token_loses_an_entry_the_owner_no_longer_holds() {
    let config = JwtConfig::for_tests_with_signing();
    let db = Db::connect("").await.unwrap();
    let users = UserDb::init(db.clone()).await;
    seed_user(&db, "alice", &[("conveyor", "read")]).await;
    let (_, secret) = store(&db, "alice", &["warehouse:write", "conveyor:read"]).await;

    let resp = app_token_grant(&request(Some(&secret)), &config, &db, &users).await;
    let claims = claims_of(resp, &config).await;
    assert!(!claims.can("warehouse", "write"));
    assert!(claims.can("conveyor", "read"));
}
