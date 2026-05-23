use crate::steps::common::WarehouseWorld;
use cucumber::{then, when};

#[when(expr = "a login attempt is made with username {string} and password {string}")]
async fn login_attempt(world: &mut WarehouseWorld, username: String, password: String) {
    let url = format!("{}/ui/login", world.api_url);

    // We use a client that doesn't follow redirects so we can check the Location header
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let res = client
        .post(&url)
        .body(format!("username={}&password={}", username, password))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send()
        .await
        .expect("Failed to send login request");

    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[then(expr = "the response should be a redirect to {string}")]
async fn check_redirect(world: &mut WarehouseWorld, expected_location: String) {
    let status = world.last_status.expect("No response status");
    assert!(
        status == 302 || status == 303 || status == 307 || status == 308,
        "Expected redirect status, got {}",
        status
    );

    let location = world
        .last_response_headers
        .get("location")
        .expect("No location header in response")
        .to_str()
        .expect("Invalid location header");

    assert!(
        location.ends_with(&expected_location),
        "Expected redirect to {}, got {}",
        expected_location,
        location
    );
}

#[then("a session cookie should be set")]
async fn check_session_cookie(world: &mut WarehouseWorld) {
    let cookie = world
        .last_response_headers
        .get("set-cookie")
        .expect("No set-cookie header in response")
        .to_str()
        .expect("Invalid set-cookie header");

    assert!(
        cookie.contains("ui_session"),
        "Cookie does not contain ui_session: {}",
        cookie
    );
}
