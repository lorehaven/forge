//! Steps for the protected-page checks: what a service does with a missing,
//! malformed or unacceptable token.
//!
//! These live under `warehouse/` because that is where they were written, but
//! switchboard's `ui_auth.feature` uses them too, so the URL comes from
//! `world.target_url()` rather than warehouse's. Hardcoding warehouse meant a
//! switchboard-only run tried to reach a port nothing was listening on.

use crate::world::ForgeWorld;
use chrono::Utc;
use cucumber::when;
use jsonwebtoken::{EncodingKey, Header, encode};
use reqwest::header::COOKIE;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    service: String,
    scope: String,
    exp: usize,
    iat: usize,
}

fn create_token(
    sub: &str,
    service: &str,
    scope: &str,
    secret: &[u8],
    duration_secs: i64,
) -> String {
    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = (now + chrono::Duration::seconds(duration_secs)).timestamp() as usize;

    let claims = Claims {
        sub: sub.to_string(),
        service: service.to_string(),
        scope: scope.to_string(),
        exp,
        iat,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .expect("Failed to encode token")
}

#[when(expr = "a GET request is sent to protected page {string} without token")]
async fn get_without_token(world: &mut ForgeWorld, page: String) {
    let url = format!("{}{}", world.target_url(), page);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send GET request");

    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[when(expr = "a GET request is sent to protected page {string} with malformed token")]
async fn get_with_malformed_token(world: &mut ForgeWorld, page: String) {
    let url = format!("{}{}", world.target_url(), page);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(&url)
        .header(COOKIE, "forge_session=not-a-valid-token".to_string())
        .send()
        .await
        .expect("Failed to send GET request");

    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[when(
    expr = "a GET request is sent to protected page {string} with token signed with wrong secret"
)]
async fn get_with_wrong_secret(world: &mut ForgeWorld, page: String) {
    let token = create_token("admin", "warehouse", "admin", b"wrong-secret", 3600);
    let url = format!("{}{}", world.target_url(), page);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(&url)
        .header(COOKIE, format!("forge_session={}", token))
        .send()
        .await
        .expect("Failed to send GET request");

    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[when(expr = "a GET request is sent to protected page {string} with expired token")]
async fn get_with_expired_token(world: &mut ForgeWorld, page: String) {
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
    let token = create_token("admin", "warehouse", "admin", jwt_secret.as_bytes(), -3600);
    let url = format!("{}{}", world.target_url(), page);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(&url)
        .header(COOKIE, format!("forge_session={}", token))
        .send()
        .await
        .expect("Failed to send GET request");

    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[when(expr = "a GET request is sent to protected page {string} with token for service {string}")]
async fn get_with_wrong_service(world: &mut ForgeWorld, page: String, service: String) {
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
    let token = create_token("admin", &service, "admin", jwt_secret.as_bytes(), 3600);
    let url = format!("{}{}", world.target_url(), page);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(&url)
        .header(COOKIE, format!("forge_session={}", token))
        .send()
        .await
        .expect("Failed to send GET request");

    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[when(expr = "a GET request is sent to protected page {string} with token with future iat")]
async fn get_with_future_iat(world: &mut ForgeWorld, page: String) {
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());

    let now = Utc::now();
    let iat = (now + chrono::Duration::seconds(300)).timestamp() as usize; // 5 minutes in future
    let exp = (now + chrono::Duration::seconds(3600)).timestamp() as usize;

    let claims = Claims {
        sub: "admin".to_string(),
        service: "warehouse".to_string(),
        scope: "admin".to_string(),
        exp,
        iat,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .expect("Failed to encode token");

    let url = format!("{}{}", world.target_url(), page);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(&url)
        .header(COOKIE, format!("forge_session={}", token))
        .send()
        .await
        .expect("Failed to send GET request");

    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}
