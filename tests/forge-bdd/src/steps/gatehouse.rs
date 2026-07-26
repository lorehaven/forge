//! Steps for the auth service: the token API and the realm login page.

use crate::world::ForgeWorld;
use cucumber::{given, then, when};
use serde_json::{Value, json};

/// Cookie names are realm-wide; a service-specific name here would mean SSO is
/// not actually happening.
const SESSION_COOKIE: &str = "forge_session";
const REFRESH_COOKIE: &str = "forge_refresh";

fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("http client")
}

// ---------------------------------------------------------------------------
// Token API
// ---------------------------------------------------------------------------

#[when(expr = "I log in with username {string} and password {string}")]
async fn login(world: &mut ForgeWorld, username: String, password: String) {
    let url = format!("{}/api/v1/auth/login", world.gatehouse_url);
    let res = world
        .client
        .post(&url)
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .expect("login request failed");

    world.record_response(res).await;
    remember_tokens(world);
}

#[given(expr = "I am logged in as {string}")]
async fn logged_in(world: &mut ForgeWorld, username: String) {
    let password = world.password.clone();
    login(world, username, password).await;
    assert_eq!(
        world.last_status,
        Some(200),
        "login failed: {:?}",
        world.last_body
    );
}

/// Keeps the tokens from the last login/refresh so later steps can present
/// them.
fn remember_tokens(world: &mut ForgeWorld) {
    let Some(json) = world.last_json.clone() else {
        return;
    };
    if let Some(access) = json.get("access_token").and_then(Value::as_str) {
        world.access_token = Some(access.to_string());
    }
    if let Some(refresh) = json.get("refresh_token").and_then(Value::as_str) {
        world.refresh_token = Some(refresh.to_string());
    }
}

#[when("I request userinfo with the access token")]
async fn userinfo_with_token(world: &mut ForgeWorld) {
    let url = format!("{}/api/v1/auth/userinfo", world.gatehouse_url);
    let token = world.access_token.clone().expect("no access token");
    let res = world
        .client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .expect("userinfo request failed");
    world.record_response(res).await;
}

#[when("I request userinfo without a token")]
async fn userinfo_without_token(world: &mut ForgeWorld) {
    let url = format!("{}/api/v1/auth/userinfo", world.gatehouse_url);
    let res = world
        .client
        .get(&url)
        .send()
        .await
        .expect("userinfo request failed");
    world.record_response(res).await;
}

#[when("I refresh the session")]
async fn refresh_session(world: &mut ForgeWorld) {
    let previous = world.refresh_token.clone().expect("no refresh token");
    world.session_cookie = Some(previous.clone()); // reused below as "the previous token"

    let url = format!("{}/api/v1/auth/refresh", world.gatehouse_url);
    let res = world
        .client
        .post(&url)
        .json(&json!({ "refresh_token": previous }))
        .send()
        .await
        .expect("refresh request failed");
    world.record_response(res).await;
    remember_tokens(world);
}

#[when("I refresh with the previous refresh token")]
async fn refresh_with_previous(world: &mut ForgeWorld) {
    let previous = world
        .session_cookie
        .clone()
        .expect("no previous refresh token recorded");
    let url = format!("{}/api/v1/auth/refresh", world.gatehouse_url);
    let res = world
        .client
        .post(&url)
        .json(&json!({ "refresh_token": previous }))
        .send()
        .await
        .expect("refresh request failed");
    world.record_response(res).await;
}

#[when("I log out")]
async fn logout(world: &mut ForgeWorld) {
    let url = format!("{}/api/v1/auth/logout", world.gatehouse_url);
    let token = world.refresh_token.clone().expect("no refresh token");
    let res = world
        .client
        .post(&url)
        .json(&json!({ "refresh_token": token }))
        .send()
        .await
        .expect("logout request failed");
    world.record_response(res).await;
}

#[then(expr = "the access token should be valid for {string}")]
async fn token_audience(world: &mut ForgeWorld, service: String) {
    let claims = decode_claims(world);
    let audiences: Vec<String> = claims
        .get("aud")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    assert!(
        audiences.iter().any(|audience| audience == &service),
        "token audiences {audiences:?} do not include '{service}'"
    );
}

#[then("the access token should carry a session id")]
async fn token_session_id(world: &mut ForgeWorld) {
    let claims = decode_claims(world);
    assert!(
        claims.get("sid").and_then(Value::as_str).is_some(),
        "token has no sid claim, so it cannot be revoked: {claims}"
    );
}

/// Reads the claims without verifying the signature - the suite is asserting
/// what gatehouse put in the token, not re-implementing verification.
fn decode_claims(world: &ForgeWorld) -> Value {
    use base64::Engine;

    let token = world.access_token.as_ref().expect("no access token");
    let payload = token.split('.').nth(1).expect("malformed JWT");
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("JWT payload is not base64");
    serde_json::from_slice(&decoded).expect("JWT payload is not JSON")
}

#[given("I am signed in to the realm")]
async fn signed_in(world: &mut ForgeWorld) {
    let username = world.username.clone();
    let password = world.password.clone();
    submit_form(world, &username, &password, None).await;
    assert!(
        world.session_cookie.is_some(),
        "login did not set a realm cookie: {:?}",
        world.last_status
    );
}

/// Redirects are not followed here: whether an unauthenticated visit bounces to
/// the login page is exactly what some of these scenarios assert.
#[when("I open the home page")]
async fn open_home(world: &mut ForgeWorld) {
    let url = format!("{}/ui/home", world.gatehouse_url);
    let mut request = no_redirect_client().get(&url);
    if let Some(cookie) = &world.session_cookie {
        request = request.header(
            reqwest::header::COOKIE,
            format!("{SESSION_COOKIE}={cookie}"),
        );
    }
    let res = request.send().await.expect("home request failed");
    world.record_response(res).await;
}

#[when("I open the base path")]
async fn open_base_path(world: &mut ForgeWorld) {
    let res = no_redirect_client()
        .get(&world.gatehouse_url)
        .send()
        .await
        .expect("base path request failed");
    world.record_response(res).await;
}

#[when("I open the UI root")]
async fn open_ui_root(world: &mut ForgeWorld) {
    let url = format!("{}/ui", world.gatehouse_url);
    let mut request = no_redirect_client().get(&url);
    if let Some(cookie) = &world.session_cookie {
        request = request.header(
            reqwest::header::COOKIE,
            format!("{SESSION_COOKIE}={cookie}"),
        );
    }
    let res = request.send().await.expect("ui root request failed");
    world.record_response(res).await;
}

#[then("the refresh token should have changed")]
async fn refresh_rotated(world: &mut ForgeWorld) {
    let previous = world.session_cookie.as_ref().expect("no previous token");
    let current = world.refresh_token.as_ref().expect("no current token");
    assert_ne!(previous, current, "refresh token was not rotated");
}

// ---------------------------------------------------------------------------
// Login page
// ---------------------------------------------------------------------------

#[when(expr = "I submit the login form with username {string} and password {string}")]
async fn submit_login_form(world: &mut ForgeWorld, username: String, password: String) {
    submit_form(world, &username, &password, None).await;
}

#[when(expr = "I submit the login form with redirect {string}")]
async fn submit_login_form_with_redirect(world: &mut ForgeWorld, redirect: String) {
    let username = world.username.clone();
    let password = world.password.clone();
    submit_form(world, &username, &password, Some(&redirect)).await;
}

async fn submit_form(
    world: &mut ForgeWorld,
    username: &str,
    password: &str,
    redirect: Option<&str>,
) {
    let url = format!("{}/ui/login", world.gatehouse_url);
    let mut form = vec![("username", username), ("password", password)];
    if let Some(redirect) = redirect {
        form.push(("redirect", redirect));
    }

    let res = no_redirect_client()
        .post(&url)
        .form(&form)
        .send()
        .await
        .expect("login form submission failed");

    world.session_cookie = cookie_value(res.headers(), SESSION_COOKIE);
    world.refresh_cookie = cookie_value(res.headers(), REFRESH_COOKIE);
    world.record_response(res).await;
}

#[when("I visit the logout page")]
async fn visit_logout(world: &mut ForgeWorld) {
    let url = format!("{}/ui/logout", world.gatehouse_url);
    let cookie = world.session_cookie.clone().unwrap_or_default();
    let res = no_redirect_client()
        .get(&url)
        .header(
            reqwest::header::COOKIE,
            format!("{SESSION_COOKIE}={cookie}"),
        )
        .send()
        .await
        .expect("logout request failed");

    world.session_cookie = cookie_value(res.headers(), SESSION_COOKIE);
    world.record_response(res).await;
}

#[then("a realm session cookie should be set")]
async fn realm_cookie_set(world: &mut ForgeWorld) {
    let cookies = set_cookie_header(world);
    assert!(
        cookies.contains(SESSION_COOKIE),
        "no {SESSION_COOKIE} cookie in: {cookies}"
    );
    assert!(
        cookies.contains(REFRESH_COOKIE),
        "no {REFRESH_COOKIE} cookie in: {cookies}"
    );
}

#[then("no realm session cookie should be set")]
async fn realm_cookie_not_set(world: &mut ForgeWorld) {
    let cookies = set_cookie_header(world);
    assert!(
        !cookies.contains(SESSION_COOKIE),
        "unexpected session cookie: {cookies}"
    );
}

/// `SameSite=Lax` and a root path are what let the cookie survive the redirect
/// back from gatehouse to a relying party.
#[then("the session cookie should be scoped to the whole site")]
async fn cookie_scope(world: &mut ForgeWorld) {
    let cookies = set_cookie_header(world);
    assert!(
        cookies.contains("Path=/"),
        "cookie is not site-wide: {cookies}"
    );
    assert!(
        cookies.contains("SameSite=Lax"),
        "cookie would be dropped on the redirect back: {cookies}"
    );
    assert!(
        cookies.contains("HttpOnly"),
        "cookie is script-readable: {cookies}"
    );
}

#[then("the realm session cookie should be cleared")]
async fn cookie_cleared(world: &mut ForgeWorld) {
    let cookies = set_cookie_header(world);
    assert!(
        cookies.contains(&format!("{SESSION_COOKIE}=;"))
            || cookies.contains("Max-Age=0")
            || cookies.contains(&format!("{SESSION_COOKIE}=\"\"")),
        "session cookie was not cleared: {cookies}"
    );
}

fn set_cookie_header(world: &ForgeWorld) -> String {
    world
        .last_response_headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>()
        .join("; ")
}

fn cookie_value(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|cookie| {
            cookie
                .split(';')
                .next()?
                .strip_prefix(&format!("{name}="))
                .map(str::to_string)
        })
}
