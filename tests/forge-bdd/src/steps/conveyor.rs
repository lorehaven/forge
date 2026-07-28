//! Steps for the CI service.
//!
//! Conveyor's queue needs Postgres, and this suite runs on an in-memory store
//! by design. These scenarios therefore cover what no database can change: the
//! UI shell, gatehouse delegation, which routes need a token, and the webhook
//! endpoint's refusals. Everything that touches the queue is covered by
//! `docker/conveyor-service/tests/integration`, against a real Postgres.

use crate::world::ForgeWorld;
use cucumber::{given, then, when};
use serde_json::json;

/// The secret `services.rs` starts conveyor with.
const WEBHOOK_SECRET: &str = "conveyor-bdd-secret";

fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("http client")
}

/// The signature conveyor expects, computed the way GitHub computes it.
fn sign(body: &str, secret: &str) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("any key length");
    mac.update(body.as_bytes());
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

fn push_body(slug: &str, git_ref: &str) -> String {
    let (owner, name) = slug.split_once('/').unwrap_or(("nobody", "unknown"));
    json!({
        "ref": git_ref,
        "after": "a".repeat(40),
        "deleted": false,
        "head_commit": { "message": "a commit" },
        "repository": {
            "name": name,
            "full_name": slug,
            "owner": { "login": owner }
        }
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

#[given("conveyor API is available")]
async fn available(world: &mut ForgeWorld) {
    world.target = crate::world::Target::Conveyor;
}

/// Follows nothing, so a redirect can be asserted on rather than chased.
#[when(expr = "I open the conveyor path {string}")]
async fn open_path(world: &mut ForgeWorld, path: String) {
    let url = format!("{}{path}", world.conveyor_url);
    let response = no_redirect_client()
        .get(&url)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url}: {e}"));
    world.record_response(response).await;
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

/// A realm token, not Basic auth.
///
/// Conveyor is a relying party: gatehouse owns the users, and conveyor never
/// seeds any of its own. There is no account here to send a password for, so
/// the suite mints a token with the realm's shared signing key - which is
/// exactly what conveyor verifies locally on every request.
#[given("I am authenticated against conveyor")]
async fn authenticated(world: &mut ForgeWorld) {
    world.access_token = Some(conveyor_token(&realm_secret()));
}

fn realm_secret() -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| "test_secret_key".to_string())
}

fn conveyor_token(secret: &str) -> String {
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde::Serialize;

    #[derive(Serialize)]
    struct Claims {
        sub: String,
        aud: Vec<String>,
        service: String,
        scope: String,
        iat: usize,
        exp: usize,
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as usize)
        .unwrap_or_default();

    encode(
        &Header::default(),
        &Claims {
            sub: "conveyor-bdd".to_string(),
            aud: vec!["conveyor".to_string()],
            service: "conveyor".to_string(),
            scope: "admin".to_string(),
            iat: now,
            exp: now + 3600,
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("sign a realm token")
}

/// The generic `GET request is sent to` step applies Basic auth only; this one
/// carries the token.
#[when(expr = "an authenticated GET is sent to {string}")]
async fn authenticated_get(world: &mut ForgeWorld, path: String) {
    let url = format!("{}{path}", world.conveyor_url);
    let mut request = no_redirect_client().get(&url);
    if let Some(token) = &world.access_token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url}: {e}"));
    world.record_response(response).await;
}

// ---------------------------------------------------------------------------
// Webhooks
// ---------------------------------------------------------------------------

async fn deliver(world: &mut ForgeWorld, event: &str, body: String, signature: Option<String>) {
    let url = format!("{}/api/v1/webhooks/github", world.conveyor_url);

    let mut request = no_redirect_client()
        .post(&url)
        .header("X-GitHub-Event", event)
        .header("X-GitHub-Delivery", uuid_like())
        .header("Content-Type", "application/json")
        .body(body);

    if let Some(signature) = signature {
        request = request.header("X-Hub-Signature-256", signature);
    }

    let response = request
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url}: {e}"));
    world.record_response(response).await;
}

/// A delivery id has to be unique, or the second one is deduplicated rather
/// than judged on its own.
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!(
        "bdd-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    )
}

#[when("a github delivery is sent with no signature")]
async fn no_signature(world: &mut ForgeWorld) {
    let body = push_body("nobody/unregistered", "refs/heads/master");
    deliver(world, "push", body, None).await;
}

#[when("a github delivery is sent with a bad signature")]
async fn bad_signature(world: &mut ForgeWorld) {
    let body = push_body("nobody/unregistered", "refs/heads/master");
    let signature = sign(&body, "not-the-secret");
    deliver(world, "push", body, Some(signature)).await;
}

#[when(expr = "a signed github push is sent for {string}")]
async fn signed_push(world: &mut ForgeWorld, slug: String) {
    let body = push_body(&slug, "refs/heads/master");
    let signature = sign(&body, WEBHOOK_SECRET);
    deliver(world, "push", body, Some(signature)).await;
}

#[when(expr = "a signed github push is sent for {string} with ref {string}")]
async fn signed_push_with_ref(world: &mut ForgeWorld, slug: String, git_ref: String) {
    let body = push_body(&slug, &git_ref);
    let signature = sign(&body, WEBHOOK_SECRET);
    deliver(world, "push", body, Some(signature)).await;
}

#[when("a signed github ping is sent")]
async fn signed_ping(world: &mut ForgeWorld) {
    let body = json!({ "zen": "Design for failure." }).to_string();
    let signature = sign(&body, WEBHOOK_SECRET);
    deliver(world, "ping", body, Some(signature)).await;
}

#[when(expr = "a delivery is sent to the {string} webhook endpoint")]
async fn unknown_provider(world: &mut ForgeWorld, provider: String) {
    let url = format!("{}/api/v1/webhooks/{provider}", world.conveyor_url);
    let response = no_redirect_client()
        .post(&url)
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url}: {e}"));
    world.record_response(response).await;
}

/// The inverse assertion, for the cases where the interesting thing is which
/// answer did *not* come back - a webhook that must not be behind the realm's
/// middleware, or a token that must have got past it.
#[then(expr = "the response status should not be {int}")]
async fn status_is_not(world: &mut ForgeWorld, unwanted: u16) {
    assert_ne!(
        world.last_status,
        Some(unwanted),
        "body was {:?}",
        world.last_body
    );
}
