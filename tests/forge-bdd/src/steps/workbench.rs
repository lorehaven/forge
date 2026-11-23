//! Steps for the task management service.
//!
//! Workbench's domain layer needs Postgres, and this suite runs on an
//! in-memory store by design. These scenarios therefore cover what no
//! database can change: the UI shell, gatehouse delegation, and which routes
//! need a token. Everything that touches the tables is covered by
//! `services/workbench/service/tests/integration`, against a real Postgres.

use crate::world::ForgeWorld;
use quench_test_bdd::{given, when};

fn no_redirect_client() -> quench_client::transport::Client {
    quench_client::transport::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(quench_client::transport::Redirect::None)
        .build()
        .expect("http client")
}

#[given("workbench API is available")]
const fn available(world: &mut ForgeWorld) {
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
    let scope = world.resolve_placeholders(&scope);
    // An issue's reporter points at their row in `auth.users`, which gatehouse writes in the estate.
    if crate::services::database_url().is_some() {
        crate::services::ensure_user("workbench-bdd").await;
    }
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
    let url = format!(
        "{}{}",
        world.workbench_url,
        world.resolve_placeholders(&path)
    );
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
async fn authenticated_post(world: &mut ForgeWorld, path: String, step: &quench_test_bdd::Step) {
    let body = step.docstring().expect("step must have a docstring");
    let url = format!(
        "{}{}",
        world.workbench_url,
        world.resolve_placeholders(&path)
    );
    let mut request = no_redirect_client()
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

/// Creates a project as an administrator and remembers its id as `{project_id}`; the caller is left holding no token, so the
/// scenario names who it acts as next.
#[given(expr = "a workbench project {string} exists")]
async fn project_exists(world: &mut ForgeWorld, key: String) {
    authenticated_with_scope(world, "admin".to_string()).await;
    let url = format!("{}/api/v1/projects", world.workbench_url);
    let response = no_redirect_client()
        .post(&url)
        .header("Content-Type", "application/json")
        .body(format!("{{\"key\":\"{key}\",\"name\":\"Project {key}\"}}"))
        .bearer_auth(world.access_token.as_deref().unwrap_or_default())
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url}: {e}"));
    let status = response.status().as_u16();
    world.record_response(response).await;
    assert!(
        matches!(status, 201 | 409),
        "creating project {key}: {status} {:?}",
        world.last_body
    );
    // A project that was there already (a rerun against the same database) is looked up by its key.
    if status == 409 {
        let url = format!("{}/api/v1/projects", world.workbench_url);
        let list = no_redirect_client()
            .get(&url)
            .bearer_auth(world.access_token.as_deref().unwrap_or_default())
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {url}: {e}"));
        let body: quench_serde::Value =
            quench_serde::json::from_str(&list.text().await.unwrap_or_default())
                .expect("a project list");
        let id = body
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p.get("key").and_then(quench_serde::Value::as_str) == Some(key.as_str()))
            .and_then(|p| p.get("id").and_then(quench_serde::Value::as_str))
            .expect("the existing project")
            .to_string();
        world.workbench_project = Some(id);
    } else {
        world.workbench_project.clone_from(&world.last_id);
    }
    world.access_token = None;
}
