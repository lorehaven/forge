//! Steps for the generic artifacts API and its `/api/v1/apk` alias.
//!
//! `routers::artifacts::scope` wraps the scope in `RequireWrite` + `Auth`.
//! `RequireWrite` only gates the mutating methods (PUT/DELETE), so a GET needs
//! nothing beyond a valid realm identity for this service, while a publish
//! needs the `write` action. `I hold a token scoped {string}` / `I hold no
//! token` are shared with the files steps (`src/steps/warehouse/files.rs`).

use crate::world::ForgeWorld;
use cucumber::when;

async fn send(
    world: &mut ForgeWorld,
    method: reqwest::Method,
    path: &str,
    body: Option<&'static [u8]>,
) {
    let url = format!("{}{path}", world.warehouse_url);
    let mut builder = world.client.request(method, &url);
    if let Some(token) = &world.token {
        builder = builder.bearer_auth(token);
    }
    if let Some(body) = body {
        builder = builder.body(body);
    }
    let res = builder.send().await.expect("artifacts request failed");
    world.record_response(res).await;
}

#[when("I request the artifacts catalog")]
async fn artifacts_catalog(world: &mut ForgeWorld) {
    send(world, reqwest::Method::GET, "/api/v1/artifacts", None).await;
}

#[when(expr = "I request artifact metadata for {string}")]
async fn artifact_metadata(world: &mut ForgeWorld, spec: String) {
    send(
        world,
        reqwest::Method::GET,
        &format!("/api/v1/artifacts/{spec}"),
        None,
    )
    .await;
}

#[when(expr = "I publish artifact {string}")]
async fn publish_artifact(world: &mut ForgeWorld, spec: String) {
    send(
        world,
        reqwest::Method::PUT,
        &format!("/api/v1/artifacts/{spec}"),
        Some(b"bdd artifact bytes"),
    )
    .await;
}

#[when("I request the apk catalog")]
async fn apk_catalog(world: &mut ForgeWorld) {
    send(world, reqwest::Method::GET, "/api/v1/apk", None).await;
}
