//! Steps for the CI service.
//!
//! Conveyor's queue needs Postgres, and this suite runs on an in-memory store
//! by design. These scenarios therefore cover what no database can change: the
//! UI shell, gatehouse delegation, which routes need a token, and the webhook
//! endpoint's refusals. Everything that touches the queue is covered by
//! `docker/conveyor-service/tests/integration`, against a real Postgres.

use crate::world::ForgeWorld;
use cucumber::{given, when};
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

/// A real realm token, minted off gatehouse rather than a shared secret.
///
/// Conveyor is a relying party: gatehouse owns the users, and conveyor never
/// seeds any of its own. There is no account here to send a password for, so
/// the suite asks gatehouse's test-mint endpoint for one instead - see
/// `world::mint_test_token`.
#[given("I am authenticated against conveyor")]
async fn authenticated(world: &mut ForgeWorld) {
    world.access_token = Some(
        crate::world::mint_test_token(
            &world.client,
            &world.gatehouse_url,
            "conveyor-bdd",
            &["conveyor"],
            "admin",
        )
        .await,
    );
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

// The signature is verified only *after* the repository the delivery names is
// looked up (its secret is per repository), and that lookup needs Postgres,
// which this suite deliberately does not provide. So "no signature" / "bad
// signature" cannot be exercised here - those twelve scenarios live in
// `docker/conveyor-service/tests/integration/webhook_tests.rs`. What this suite
// covers is everything the handler decides *before* the lookup: the provider,
// the event type, the body's shape and the ref.

/// Delivers with the given event type and raw body, unsigned - for the checks
/// that run before any signature or repository lookup.
#[when(expr = "a github {string} delivery is sent with body {string}")]
async fn github_event_with_body(world: &mut ForgeWorld, event: String, body: String) {
    deliver(world, &event, body, None).await;
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
