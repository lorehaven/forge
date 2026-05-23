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
    assert_eq!(world.last_status, Some(302));
    let location = world
        .last_response_headers
        .get("location")
        .expect("No location header")
        .to_str()
        .expect("Invalid location header");
    assert_eq!(location, expected_path);
}

#[then("session cookie should be set")]
async fn check_session_cookie(world: &mut SwitchboardWorld) {
    let set_cookie = world
        .last_response_headers
        .get("set-cookie")
        .expect("No set-cookie header")
        .to_str()
        .expect("Invalid set-cookie header");
    assert!(set_cookie.contains("switchboard_ui_session="));
}
