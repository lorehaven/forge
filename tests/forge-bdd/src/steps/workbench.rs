//! Steps for the task management service.
//!
//! Workbench's domain layer needs Postgres, and this suite runs on an
//! in-memory store by design. These scenarios therefore cover what no
//! database can change: the UI shell, gatehouse delegation, and which routes
//! need a token. Everything that touches the tables is covered by
//! `docker/workbench-service/tests/integration`, against a real Postgres.

use crate::world::ForgeWorld;
use cucumber::{given, when};

fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("http client")
}

#[given("workbench API is available")]
async fn available(world: &mut ForgeWorld) {
    world.target = crate::world::Target::Workbench;
}

/// Follows nothing, so a redirect can be asserted on rather than chased.
#[when(expr = "I open the workbench path {string}")]
async fn open_path(world: &mut ForgeWorld, path: String) {
    let url = format!("{}{path}", world.workbench_url);
    let response = no_redirect_client()
        .get(&url)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url}: {e}"));
    world.record_response(response).await;
}

/// A real realm token, minted off gatehouse rather than a shared secret.
///
/// Workbench is a relying party: gatehouse owns the users, and workbench
/// never seeds any of its own. There is no account here to send a password
/// for, so the suite asks gatehouse's test-mint endpoint for one instead -
/// see `world::mint_test_token`.
#[given(expr = "I am authenticated against workbench with scope {string}")]
async fn authenticated_with_scope(world: &mut ForgeWorld, scope: String) {
    world.access_token = Some(
        crate::world::mint_test_token(
            &world.client,
            &world.gatehouse_url,
            "workbench-bdd",
            &["workbench"],
            &scope,
        )
        .await,
    );
}

#[given("I am authenticated against workbench")]
async fn authenticated(world: &mut ForgeWorld) {
    authenticated_with_scope(world, "admin".to_string()).await;
}

/// The generic `GET request is sent to` step applies Basic auth only; this one
/// carries the bearer token, and follows nothing - the same reason
/// `open_path` doesn't. Named "GET request" rather than conveyor's "GET is
/// sent" so the two step patterns cannot collide - cucumber resolves step
/// text globally across every service's steps, not per-file.
#[when(expr = "an authenticated GET request is sent to {string}")]
async fn authenticated_get(world: &mut ForgeWorld, path: String) {
    let url = format!("{}{path}", world.workbench_url);
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

#[when(expr = "an authenticated POST is sent to {string} with body:")]
async fn authenticated_post(world: &mut ForgeWorld, path: String, step: &cucumber::gherkin::Step) {
    let body = step.docstring().expect("step must have a docstring");
    let url = format!("{}{path}", world.workbench_url);
    let mut request = no_redirect_client()
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
