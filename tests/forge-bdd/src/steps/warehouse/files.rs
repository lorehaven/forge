//! Steps for the files API: plain storage, and the one place in warehouse where
//! a realm permission's read/write level is actually enforced (`RequireWrite`,
//! mounted on `routers::files::scope`). The BDD harness configures a `test`
//! storage for exactly this - see `FILE_STORAGES` in `services.rs`.

use crate::world::{ForgeWorld, mint_test_token};
use cucumber::{given, then, when};

#[given(expr = "I hold a token scoped {string}")]
async fn hold_token(world: &mut ForgeWorld, scope: String) {
    world.token = Some(
        mint_test_token(&world.client, &world.gatehouse_url, "bdd-user", &["warehouse"], &scope).await,
    );
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
