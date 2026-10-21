//! Daybook scenarios use real Gatehouse-signed tokens and the service's
//! in-memory DB adapter. Postgres CRUD/ownership behavior lives in the
//! daybook-service integration suite.
use crate::world::ForgeWorld;
use cucumber::{given, when};

#[given("daybook API is available")]
async fn available(world: &mut ForgeWorld) {
    world.target = crate::world::Target::Daybook;
}

#[given(expr = "I am authenticated against daybook with scope {string}")]
async fn authenticated_with_scope(world: &mut ForgeWorld, scope: String) {
    world.access_token = Some(
        crate::world::mint_test_token(
            &world.client,
            &world.gatehouse_url,
            "daybook-bdd-user",
            &["daybook"],
            &scope,
        )
        .await,
    );
}

#[given(expr = "I have a Gatehouse token with audience {string}")]
async fn authenticated_with_audience(world: &mut ForgeWorld, audience: String) {
    world.access_token = Some(
        crate::world::mint_test_token(
            &world.client,
            &world.gatehouse_url,
            "daybook-bdd-user",
            &[&audience],
            "admin",
        )
        .await,
    );
}

#[when(expr = "an authenticated Daybook POST is sent to {string} with body:")]
async fn authenticated_post(world: &mut ForgeWorld, path: String, step: &cucumber::gherkin::Step) {
    let body = step.docstring().expect("step must have a docstring");
    let url = format!("{}{path}", world.daybook_url);
    let mut request = world
        .client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string());
    if let Some(token) = &world.access_token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url}: {e}"));
    world.record_response(response).await;
}
