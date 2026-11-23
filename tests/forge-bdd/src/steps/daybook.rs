//! Daybook scenarios use real Gatehouse-signed tokens and the service's
//! in-memory DB adapter. Postgres CRUD/ownership behavior lives in the
//! daybook-service integration suite.
use crate::world::ForgeWorld;
use quench_test_bdd::{given, when};

#[given("daybook API is available")]
const fn available(world: &mut ForgeWorld) {
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

/// A second (third, ...) person: the token is for `user`, who owns what they create and sees nothing of anyone else's.
#[given(expr = "I am authenticated against daybook as {string}")]
async fn authenticated_as(world: &mut ForgeWorld, user: String) {
    // Their tasks point at their row in `auth.users`.
    if crate::services::database_url().is_some() {
        crate::services::ensure_user(&user).await;
    }
    world.access_token = Some(
        crate::world::mint_test_token(
            &world.client,
            &world.gatehouse_url,
            &user,
            &["daybook"],
            "user daybook:read daybook:write",
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
async fn authenticated_post(world: &mut ForgeWorld, path: String, step: &quench_test_bdd::Step) {
    let body = step.docstring().expect("step must have a docstring");
    let url = format!("{}{path}", world.daybook_url);
    let mut request = world
        .client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body.clone());
    if let Some(token) = &world.access_token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url}: {e}"));
    world.record_response(response).await;
}

/// The same as the POST step for an update; `{last_id}` is the id of what was created last.
#[when(expr = "an authenticated Daybook PUT is sent to {string} with body:")]
async fn authenticated_put(world: &mut ForgeWorld, path: String, step: &quench_test_bdd::Step) {
    let body = step.docstring().expect("step must have a docstring");
    let url = format!("{}{}", world.daybook_url, world.resolve_placeholders(&path));
    let mut request = world
        .client
        .put(&url)
        .header("Content-Type", "application/json")
        .body(body.clone());
    if let Some(token) = &world.access_token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .unwrap_or_else(|e| panic!("PUT {url}: {e}"));
    world.record_response(response).await;
}

/// Remembers the id of what was just created under a name, so a later step can come back to it as another person.
#[given(expr = "{string} created the task {string}")]
async fn created_task(world: &mut ForgeWorld, user: String, content: String) {
    authenticated_as(world, user).await;
    let url = format!("{}/api/v1/tasks", world.daybook_url);
    let response = world
        .client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(format!("{{\"content\":\"{content}\"}}"))
        .bearer_auth(world.access_token.as_deref().unwrap_or_default())
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url}: {e}"));
    assert_eq!(response.status().as_u16(), 201, "creating the task");
    world.record_response(response).await;
}
