//! Steps for the files API: plain storage, and the one place in warehouse where
//! a realm permission's read/write level is actually enforced (`RequireWrite`,
//! mounted on `routers::files::scope`). The BDD harness configures a `test`
//! storage for exactly this - see `FILE_STORAGES` in `services.rs`.

use crate::world::ForgeWorld;
use cucumber::{given, then, when};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    aud: Vec<String>,
    service: String,
    scope: String,
    exp: usize,
    iat: usize,
    #[serde(default)]
    sid: Option<String>,
}

/// Mints a realm-shaped token directly, the way `ui_jwt.rs` does for the same
/// reason: this suite runs warehouse alone, with no gatehouse to issue one.
/// `scope` is whatever the scenario is testing - a role, a `service:level`
/// grant, or both.
fn token(secret: &str, scope: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;

    let claims = Claims {
        sub: "bdd-user".to_string(),
        aud: vec!["warehouse".to_string()],
        service: "warehouse".to_string(),
        scope: scope.to_string(),
        exp: now + 3600,
        iat: now,
        sid: None,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("failed to encode test token")
}

/// The suite shares one signing key across every service (`services.rs`), the
/// same variable `JwtConfig::init` reads, so this is the same secret warehouse
/// is verifying against.
fn jwt_secret() -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| "forge-bdd-shared-secret".to_string())
}

#[given(expr = "I hold a token scoped {string}")]
async fn hold_token(world: &mut ForgeWorld, scope: String) {
    world.token = Some(token(&jwt_secret(), &scope));
}

#[given("I hold no token")]
async fn hold_no_token(world: &mut ForgeWorld) {
    world.token = None;
}

fn files_url(world: &ForgeWorld, path: &str) -> String {
    format!("{}/api/v1/files{path}", world.warehouse_url)
}

async fn request(
    world: &mut ForgeWorld,
    method: reqwest::Method,
    path: &str,
    body: Option<&'static [u8]>,
) {
    let url = files_url(world, path);
    let mut builder = world.client.request(method, &url);
    if let Some(token) = &world.token {
        builder = builder.bearer_auth(token);
    }
    if let Some(body) = body {
        builder = builder.body(body);
    }
    let res = builder.send().await.expect("files request failed");
    world.record_response(res).await;
}

#[when(expr = "I upload {string} to the test storage")]
async fn upload(world: &mut ForgeWorld, path: String) {
    request(
        world,
        reqwest::Method::PUT,
        &format!("/test/file?path={path}"),
        Some(b"bdd file contents"),
    )
    .await;
}

#[when(expr = "I download {string} from the test storage")]
async fn download(world: &mut ForgeWorld, path: String) {
    request(
        world,
        reqwest::Method::GET,
        &format!("/test/file?path={path}"),
        None,
    )
    .await;
}

#[when(expr = "I delete {string} from the test storage")]
async fn delete(world: &mut ForgeWorld, path: String) {
    request(
        world,
        reqwest::Method::DELETE,
        &format!("/test/file?path={path}"),
        None,
    )
    .await;
}

#[when("I list the test storage")]
async fn list(world: &mut ForgeWorld) {
    request(world, reqwest::Method::GET, "/test", None).await;
}

#[then(expr = "the test storage should contain {string}")]
async fn should_contain(world: &mut ForgeWorld, path: String) {
    // Downloads are a read, so this needs no permission beyond what the
    // Background already granted - it is here to prove the write actually
    // landed, not to test the read path again.
    download(world, path).await;
    assert_eq!(
        world.last_status,
        Some(200),
        "expected the earlier upload to be readable back: {:?}",
        world.last_body
    );
}
