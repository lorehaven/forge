use crate::steps::common::SwitchboardWorld;
use cucumber::{then, when};

#[when(expr = "login attempt is made with username {string} and password {string}")]
async fn login_attempt(world: &mut SwitchboardWorld, username: String, password: String) {
    let url = format!("{}/ui/login", world.api_url);
    let params = [("username", username), ("password", password)];

    // We want to see the redirect, so we disable automatic redirect following
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let res = client
        .post(&url)
        .form(&params)
        .send()
        .await
        .expect("Failed to send login request");

    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[then(expr = "response should be a redirect to {string}")]
async fn check_redirect(world: &mut SwitchboardWorld, expected_path: String) {
    assert_eq!(world.last_status, Some(302), "Expected redirect status 302");
    let location = world
        .last_response_headers
        .get("location")
        .expect("No location header")
        .to_str()
        .expect("Invalid location header");
    // Allow both exact match and contains match for flexibility
    assert!(
        location == expected_path || location.contains(&expected_path),
        "Expected location to be or contain '{}', got '{}'",
        expected_path,
        location
    );
}

#[then(expr = "location header contains {string}")]
async fn check_location_contains(world: &mut SwitchboardWorld, expected_substring: String) {
    let location = world
        .last_response_headers
        .get("location")
        .expect("No location header")
        .to_str()
        .expect("Invalid location header");
    assert!(
        location.contains(&expected_substring),
        "Location '{}' does not contain '{}'",
        location,
        expected_substring
    );
}

#[then("session cookie should be set")]
async fn check_session_cookie(world: &mut SwitchboardWorld) {
    let set_cookie_header = world
        .last_response_headers
        .get("set-cookie")
        .map(|h| h.to_str().unwrap_or(""))
        .unwrap_or("");

    assert!(
        !set_cookie_header.is_empty() && set_cookie_header.contains("session"),
        "No session cookie found in headers. Found: {}",
        set_cookie_header
    );
}
