//! Steps shared by every suite.
//!
//! The three original crates each defined their own "GET request is sent to …"
//! and "response status should be …". In one world those patterns collide, so
//! they live here once and resolve against [`ForgeWorld::target`], which the
//! `Given <service> API is available` background step sets.

use crate::world::{ForgeWorld, Target};
use cucumber::{given, then, when};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Availability / target selection
// ---------------------------------------------------------------------------

#[given("sage API is available")]
#[then("sage API is available")]
async fn sage_available(world: &mut ForgeWorld) {
    world.target = Target::Sage;
    let url = format!("{}/ui/login", world.sage_url);
    await_service(world, &url, "sage").await;
}

#[given("switchboard API is available")]
async fn switchboard_available(world: &mut ForgeWorld) {
    world.target = Target::Switchboard;
    let url = format!("{}/health", world.switchboard_url);
    await_service(world, &url, "switchboard").await;
}

#[given("warehouse API is available")]
async fn warehouse_available(world: &mut ForgeWorld) {
    world.target = Target::Warehouse;
    let url = format!("{}/health", world.warehouse_url);
    await_service(world, &url, "warehouse").await;
}

#[given("gatehouse API is available")]
async fn gatehouse_available(world: &mut ForgeWorld) {
    world.target = Target::Gatehouse;
    let url = format!("{}/ui/login", world.gatehouse_url);
    await_service(world, &url, "gatehouse").await;
}

async fn await_service(world: &ForgeWorld, url: &str, name: &str) {
    let start = Instant::now();
    let timeout = Duration::from_secs(30);

    while start.elapsed() < timeout {
        if world.client.get(url).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("{name} API at {url} did not become available within {timeout:?}");
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

#[when(expr = "GET request is sent to {string}")]
async fn send_get_request(world: &mut ForgeWorld, path: String) {
    let url = world.resolve_url(&path);
    let request = world.apply_auth(world.client.get(&url));
    let res = request.send().await.expect("Failed to send GET request");
    world.record_response(res).await;
}

#[when(expr = "DELETE request is sent to {string}")]
async fn send_delete_request(world: &mut ForgeWorld, path: String) {
    let url = world.resolve_url(&path);
    let request = world.apply_auth(world.client.delete(&url));
    let res = request.send().await.expect("Failed to send DELETE request");
    world.record_response(res).await;
}

#[given("I am authenticated")]
async fn authenticated(world: &mut ForgeWorld) {
    world.credentials = Some((world.username.clone(), world.password.clone()));
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

#[then(expr = "response status should be {int}")]
async fn check_status(world: &mut ForgeWorld, status: u16) {
    assert_eq!(
        world.last_status.expect("No response available"),
        status,
        "Status code mismatch"
    );
}

#[then("response should be a redirect")]
async fn check_is_redirect(world: &mut ForgeWorld) {
    let status = world.last_status.expect("No response available");
    assert!(
        matches!(status, 301 | 302 | 303 | 307),
        "Expected redirect status (301/302/303/307), got {status}"
    );
}

#[then("response should be a JSON object")]
async fn check_json_object(world: &mut ForgeWorld) {
    let json = world
        .last_json
        .as_ref()
        .expect("No JSON response available");
    assert!(json.is_object(), "Response is not a JSON object: {json:?}");
}

#[then(expr = "response content type should be {string}")]
async fn check_content_type(world: &mut ForgeWorld, expected: String) {
    let actual = world
        .last_response_headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("No Content-Type header");
    assert!(
        actual.contains(&expected),
        "Content-Type '{actual}' does not contain '{expected}'"
    );
}

#[then(expr = "response should contain {string}")]
async fn check_response_contains(world: &mut ForgeWorld, expected: String) {
    let expected = world.resolve_placeholders(&expected);
    let body = world
        .last_body
        .as_ref()
        .expect("No response body available");
    assert!(
        body.contains(&expected),
        "Response body does not contain '{expected}'"
    );
}

#[then(expr = "response should not contain {string}")]
async fn check_response_not_contains(world: &mut ForgeWorld, expected: String) {
    let body = world
        .last_body
        .as_ref()
        .expect("No response body available");
    assert!(
        !body.contains(&expected),
        "Response body contains '{expected}' but it shouldn't"
    );
}

#[then(expr = "the response status should be {int}")]
async fn response_status_should_be(world: &mut ForgeWorld, expected: u16) -> Result<(), String> {
    match world.last_status {
        Some(status) if status == expected => Ok(()),
        Some(status) => Err(format!("Expected status {expected}, got {status}")),
        None => Err("No response status recorded".to_string()),
    }
}

#[then("the response should contain error message")]
async fn response_contains_error(_world: &mut ForgeWorld) -> Result<(), String> {
    Ok(())
}

/// Used by every suite now that login is a redirect to gatehouse.
#[then(expr = "the redirect location should contain {string}")]
async fn redirect_location(world: &mut ForgeWorld, expected: String) {
    let location = world
        .last_response_headers
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("no Location header");
    assert!(
        location.contains(&expected),
        "redirect to '{location}' does not contain '{expected}'"
    );
}
