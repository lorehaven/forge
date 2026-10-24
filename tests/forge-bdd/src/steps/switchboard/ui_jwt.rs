use crate::world::ForgeWorld;
use quench_client::transport::header::COOKIE;
use quench_core::time::{Duration, Time};
use quench_crypto::jwt::{Algorithm, EncodingKey, Header, encode};
use quench_serde::{Deserialize, Serialize};
use quench_test_bdd::when;

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
    let now = Time::now();
    let iat = now.timestamp() as usize;
    let exp = (now + Duration::seconds(duration_secs)).timestamp() as usize;

    let claims = Claims {
        sub: sub.to_string(),
        service: service.to_string(),
        scope: scope.to_string(),
        exp,
        iat,
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .expect("Failed to encode token")
}

#[when(expr = "GET request is sent to protected page {string} without token")]
async fn get_without_token(world: &mut ForgeWorld, page: String) {
    let url = format!("{}{}", world.switchboard_url, page);
    let client = quench_client::transport::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(quench_client::transport::Redirect::None)
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

#[when(expr = "GET request is sent to protected page {string} with malformed token")]
async fn get_with_malformed_token(world: &mut ForgeWorld, page: String) {
    let url = format!("{}{}", world.switchboard_url, page);
    let client = quench_client::transport::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(quench_client::transport::Redirect::None)
        .build()
        .unwrap();
    let res = client
        .get(&url)
        .header(COOKIE, "forge_session=not-a-valid-token")
        .send()
        .await
        .expect("Failed to send GET request");

    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[when(expr = "GET request is sent to protected page {string} with token signed with wrong secret")]
async fn get_with_wrong_secret(world: &mut ForgeWorld, page: String) {
    let token = create_token("admin", "switchboard", "admin", b"wrong-secret", 3600);
    let url = format!("{}{}", world.switchboard_url, page);
    let client = quench_client::transport::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(quench_client::transport::Redirect::None)
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

#[when(expr = "GET request is sent to protected page {string} with expired token")]
async fn get_with_expired_token(world: &mut ForgeWorld, page: String) {
    let now = Time::now().timestamp();
    let token = crate::world::mint_test_token_at(
        &world.client,
        &world.gatehouse_url,
        "admin",
        &["switchboard"],
        "admin",
        Some(now),
        Some(now - 3600),
    )
    .await;
    let url = format!("{}{}", world.switchboard_url, page);
    let client = quench_client::transport::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(quench_client::transport::Redirect::None)
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

#[when(expr = "GET request is sent to protected page {string} with token for service {string}")]
async fn get_with_wrong_service(world: &mut ForgeWorld, page: String, service: String) {
    let token = crate::world::mint_test_token(
        &world.client,
        &world.gatehouse_url,
        "admin",
        &[&service],
        "admin",
    )
    .await;
    let url = format!("{}{}", world.switchboard_url, page);
    let client = quench_client::transport::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(quench_client::transport::Redirect::None)
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

#[when(expr = "GET request is sent to protected page {string} with token with future iat")]
async fn get_with_future_iat(world: &mut ForgeWorld, page: String) {
    let now = Time::now().timestamp();
    let token = crate::world::mint_test_token_at(
        &world.client,
        &world.gatehouse_url,
        "admin",
        &["switchboard"],
        "admin",
        Some(now + 300), // 5 minutes in the future
        Some(now + 3600),
    )
    .await;

    let url = format!("{}{}", world.switchboard_url, page);
    let client = quench_client::transport::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(quench_client::transport::Redirect::None)
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
