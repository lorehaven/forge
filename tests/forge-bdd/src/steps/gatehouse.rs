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

/// The no-session branch of `GET /api/v1/authorize`: a client the BDD harness
/// already registers (`clients.toml`), a made-up but well-formed PKCE
/// challenge, and no session cookie - exactly what a browser sends on its
/// first visit.
#[when(expr = "I request authorization for client {string} without a session")]
async fn request_authorization_without_session(world: &mut ForgeWorld, client_id: String) {
    let redirect_uri = format!("{}/ui/auth/callback", world.conveyor_url);
    let url = format!(
        "{}/api/v1/authorize?client_id={client_id}&redirect_uri={}&state=test-state&code_challenge=test-challenge&code_challenge_method=S256",
        world.gatehouse_url,
        urlencoding::encode(&redirect_uri),
    );
    let res = no_redirect_client()
        .get(&url)
        .send()
        .await
        .expect("authorize request failed");
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

// ---------------------------------------------------------------------------
// User administration
// ---------------------------------------------------------------------------
//
// The scenarios here create and delete real users, so each one cleans up after
// itself through `Given no user {string} exists` in its Background: a suite that
// left accounts behind would change what the next run of the login scenarios
// sees.

/// Signs in as the seeded admin and keeps the token aside, so a scenario can log
/// in as somebody else later and still administer the realm.
#[given("I am administering the realm")]
async fn administering(world: &mut ForgeWorld) {
    let username = world.username.clone();
    let password = world.password.clone();
    login(world, username, password).await;
    assert_eq!(
        world.last_status,
        Some(200),
        "could not sign in as the realm admin: {:?}",
        world.last_body
    );
    world.admin_token = world.access_token.clone();
}

fn admin_token(world: &ForgeWorld) -> String {
    world
        .admin_token
        .clone()
        .expect("no admin token; the scenario needs `Given I am administering the realm`")
}

#[given(expr = "no user {string} exists")]
async fn ensure_absent(world: &mut ForgeWorld, username: String) {
    let url = format!("{}/api/v1/users/{username}", world.gatehouse_url);
    let res = world
        .client
        .delete(&url)
        .bearer_auth(admin_token(world))
        .send()
        .await
        .expect("delete request failed");
    // 204 or 404 are both "it is not there now".
    assert!(
        res.status().is_success() || res.status().as_u16() == 404,
        "could not clear user {username}: {}",
        res.status()
    );
}

#[given(expr = "a user {string} with password {string} and {string} on {string}")]
async fn user_with_grant(
    world: &mut ForgeWorld,
    username: String,
    password: String,
    level: String,
    service: String,
) {
    let token = admin_token(world);
    create_user(
        world,
        &token,
        &username,
        &password,
        &["user"],
        json!({ service: [level] }),
    )
    .await;
    assert_eq!(
        world.last_status,
        Some(201),
        "could not create {username}: {:?}",
        world.last_body
    );
}

#[given(expr = "a user {string} with password {string} and no permissions")]
async fn user_without_grants(world: &mut ForgeWorld, username: String, password: String) {
    let token = admin_token(world);
    create_user(world, &token, &username, &password, &["user"], json!({})).await;
    assert_eq!(
        world.last_status,
        Some(201),
        "could not create {username}: {:?}",
        world.last_body
    );
}

#[when(expr = "I create a user {string} with password {string} and {string} on {string}")]
async fn when_create_user(
    world: &mut ForgeWorld,
    username: String,
    password: String,
    level: String,
    service: String,
) {
    let token = admin_token(world);
    create_user(
        world,
        &token,
        &username,
        &password,
        &["user"],
        json!({ service: [level] }),
    )
    .await;
}

/// Same request, presented with the caller's own token rather than the admin
/// one - how the delegated-user-manager scenarios prove a narrower grant is
/// enough, without duplicating every admin-token step.
#[when(
    expr = "I create a user {string} with password {string} and no permissions using my own token"
)]
async fn when_create_user_as_me(world: &mut ForgeWorld, username: String, password: String) {
    let token = world.access_token.clone().expect("no access token");
    create_user(world, &token, &username, &password, &["user"], json!({})).await;
}

#[when(
    expr = "I create a user {string} with password {string} and role {string} using my own token"
)]
async fn when_create_user_with_role_as_me(
    world: &mut ForgeWorld,
    username: String,
    password: String,
    role: String,
) {
    let token = world.access_token.clone().expect("no access token");
    create_user(
        world,
        &token,
        &username,
        &password,
        &[role.as_str()],
        json!({}),
    )
    .await;
}

async fn create_user(
    world: &mut ForgeWorld,
    token: &str,
    username: &str,
    password: &str,
    roles: &[&str],
    permissions: Value,
) {
    let url = format!("{}/api/v1/users", world.gatehouse_url);
    let res = world
        .client
        .post(&url)
        .bearer_auth(token)
        .json(&json!({
            "username": username,
            "password": password,
            "roles": roles,
            "permissions": permissions,
        }))
        .send()
        .await
        .expect("create user request failed");
    world.record_response(res).await;
}

#[when("I list the realm's users")]
async fn list_users(world: &mut ForgeWorld) {
    let url = format!("{}/api/v1/users", world.gatehouse_url);
    let res = world
        .client
        .get(&url)
        .bearer_auth(admin_token(world))
        .send()
        .await
        .expect("list users request failed");
    world.record_response(res).await;
}

/// Deliberately presents the *current* access token rather than the admin one:
/// this is how the suite asserts that a non-admin is refused.
#[when("I list the realm's users with my own token")]
async fn list_users_as_me(world: &mut ForgeWorld) {
    let url = format!("{}/api/v1/users", world.gatehouse_url);
    let token = world.access_token.clone().expect("no access token");
    let res = world
        .client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .expect("list users request failed");
    world.record_response(res).await;
}

#[when(expr = "I grant {string} on {string} to {string}")]
async fn grant(world: &mut ForgeWorld, level: String, service: String, username: String) {
    let url = format!(
        "{}/api/v1/users/{username}/permissions",
        world.gatehouse_url
    );
    let res = world
        .client
        .put(&url)
        .bearer_auth(admin_token(world))
        .json(&json!({ "permissions": { service: [level] } }))
        .send()
        .await
        .expect("grant request failed");
    world.record_response(res).await;
}

/// Mirrors `grant`, presented with the caller's own token - proves
/// `gatehouse:manage-permissions` is enough on its own, with no admin role
/// involved.
#[when(expr = "I grant {string} on {string} to {string} using my own token")]
async fn grant_as_me(world: &mut ForgeWorld, level: String, service: String, username: String) {
    let url = format!(
        "{}/api/v1/users/{username}/permissions",
        world.gatehouse_url
    );
    let token = world.access_token.clone().expect("no access token");
    let res = world
        .client
        .put(&url)
        .bearer_auth(token)
        .json(&json!({ "permissions": { service: [level] } }))
        .send()
        .await
        .expect("grant request failed");
    world.record_response(res).await;
}

/// Mirrors `promote`, presented with the caller's own token - this is the
/// operation `gatehouse:manage-permissions`/`edit-user` deliberately do NOT
/// cover; only the literal `admin` role may hand out `admin` or `service`.
#[when(expr = "I make {string} an admin using my own token")]
async fn promote_as_me(world: &mut ForgeWorld, username: String) {
    let url = format!("{}/api/v1/users/{username}", world.gatehouse_url);
    let token = world.access_token.clone().expect("no access token");
    let res = world
        .client
        .patch(&url)
        .bearer_auth(token)
        .json(&json!({ "roles": ["admin"] }))
        .send()
        .await
        .expect("promote request failed");
    world.record_response(res).await;
}

/// Mirrors `delete_self`'s request shape but against a *named* user with the
/// caller's own token, rather than the caller's own account - proves
/// `gatehouse:delete-user` is enough on its own.
#[when(expr = "I delete {string} using my own token")]
async fn delete_as_me(world: &mut ForgeWorld, username: String) {
    let url = format!("{}/api/v1/users/{username}", world.gatehouse_url);
    let token = world.access_token.clone().expect("no access token");
    let res = world
        .client
        .delete(&url)
        .bearer_auth(token)
        .send()
        .await
        .expect("delete request failed");
    world.record_response(res).await;
}

#[when(expr = "I apply the {string} template to {string}")]
async fn apply_template(world: &mut ForgeWorld, template: String, username: String) {
    let url = format!("{}/api/v1/users/{username}/template", world.gatehouse_url);
    let res = world
        .client
        .post(&url)
        .bearer_auth(admin_token(world))
        .json(&json!({ "template": template }))
        .send()
        .await
        .expect("apply template request failed");
    world.record_response(res).await;
}

#[when(expr = "I make {string} an admin")]
async fn promote(world: &mut ForgeWorld, username: String) {
    let url = format!("{}/api/v1/users/{username}", world.gatehouse_url);
    let res = world
        .client
        .patch(&url)
        .bearer_auth(admin_token(world))
        .json(&json!({ "roles": ["admin"] }))
        .send()
        .await
        .expect("promote request failed");
    world.record_response(res).await;
}

#[when(expr = "I remove my own admin role")]
async fn demote_self(world: &mut ForgeWorld) {
    let username = world.username.clone();
    let url = format!("{}/api/v1/users/{username}", world.gatehouse_url);
    let res = world
        .client
        .patch(&url)
        .bearer_auth(admin_token(world))
        .json(&json!({ "roles": ["user"] }))
        .send()
        .await
        .expect("demote request failed");
    world.record_response(res).await;
}

#[when("I delete my own account")]
async fn delete_self(world: &mut ForgeWorld) {
    let username = world.username.clone();
    let url = format!("{}/api/v1/users/{username}", world.gatehouse_url);
    let res = world
        .client
        .delete(&url)
        .bearer_auth(admin_token(world))
        .send()
        .await
        .expect("delete request failed");
    world.record_response(res).await;
}

#[when("I ask what I may do")]
async fn ask_my_access(world: &mut ForgeWorld) {
    let url = format!("{}/api/v1/me", world.gatehouse_url);
    let token = world.access_token.clone().expect("no access token");
    let res = world
        .client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .expect("me request failed");
    world.record_response(res).await;
}

#[then(expr = "the access token should not be valid for {string}")]
async fn token_audience_absent(world: &mut ForgeWorld, service: String) {
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
        !audiences.iter().any(|audience| audience == &service),
        "token audiences {audiences:?} should not include '{service}'"
    );
}

#[then(expr = "the access token scope should be {string}")]
async fn token_scope(world: &mut ForgeWorld, expected: String) {
    let claims = decode_claims(world);
    let scope = claims.get("scope").and_then(Value::as_str).unwrap_or("");
    assert_eq!(scope, expected, "unexpected scope in {claims}");
}

#[then(expr = "the response should report {string} on {string}")]
async fn reports_level(world: &mut ForgeWorld, level: String, service: String) {
    let body = world.last_json.clone().expect("no JSON response");
    let effective = body
        .get("effective")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let actions: Vec<&str> = effective
        .get(&service)
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    assert!(
        actions.contains(&level.as_str()),
        "expected {level} on {service} in {body}"
    );
}

/// The hash must never leave the service, whatever the response is.
///
/// On a JSON response that is tightened to "no `password` field at all", since
/// `UserView` deliberately has no such field. It cannot be a plain substring
/// check: an HTML page legitimately contains `name="password"` wherever it offers
/// a box to type a new one.
#[then("the response should never contain a password hash")]
async fn no_hashes(world: &mut ForgeWorld) {
    let body = world.last_body.clone().unwrap_or_default();
    assert!(
        !body.contains("$argon2"),
        "a password hash reached the client: {body}"
    );

    if let Some(json) = world.last_json.clone() {
        assert!(
            !mentions_password(&json),
            "the JSON response carries a password field: {json}"
        );
    }
}

/// Whether any object anywhere in the document has a `password` key.
fn mentions_password(value: &Value) -> bool {
    match value {
        Value::Object(fields) => {
            fields.contains_key("password") || fields.values().any(mentions_password)
        }
        Value::Array(items) => items.iter().any(mentions_password),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// The admin pages
// ---------------------------------------------------------------------------
//
// Driven through the forms rather than the JSON API, because what these
// scenarios are for is the pages: the guard on them, the permission matrix the
// form posts, and the rules the pages share with the API.

/// Signs in through the login form as somebody other than the seeded admin, so a
/// scenario can check what a non-admin sees.
#[given(expr = "I am signed in to the realm as {string} with password {string}")]
async fn signed_in_as(world: &mut ForgeWorld, username: String, password: String) {
    submit_form(world, &username, &password, None).await;
    assert!(
        world.session_cookie.is_some(),
        "login did not set a realm cookie for {username}: {:?}",
        world.last_status
    );
}

/// GETs a page with whatever realm cookie the scenario currently holds.
/// Redirects are not followed: whether an unauthenticated visit bounces to the
/// login form is exactly what one of these scenarios asserts.
async fn open_page(world: &mut ForgeWorld, path: &str) {
    let url = format!("{}{path}", world.gatehouse_url);
    let mut request = no_redirect_client().get(&url);
    if let Some(cookie) = &world.session_cookie {
        request = request.header(
            reqwest::header::COOKIE,
            format!("{SESSION_COOKIE}={cookie}"),
        );
    }
    let res = request.send().await.expect("page request failed");
    world.record_response(res).await;
}

#[when("I open the user administration page")]
async fn open_admin_users(world: &mut ForgeWorld) {
    open_page(world, "/ui/admin/users").await;
}

/// The pages accept a `?err=` key from the redirect that preceded them, so this
/// is how a scenario checks that an unknown one is not rendered.
#[when(expr = "I open the user administration page with error {string}")]
async fn open_admin_users_with_error(world: &mut ForgeWorld, key: String) {
    open_page(world, &format!("/ui/admin/users?err={key}")).await;
}

#[when(expr = "I open the administration page for {string}")]
async fn open_admin_user(world: &mut ForgeWorld, username: String) {
    open_page(world, &format!("/ui/admin/users/{username}")).await;
}

/// POSTs a form with the current realm cookie, following no redirects so the
/// `Location` is assertable.
async fn post_form(world: &mut ForgeWorld, path: &str, form: &[(&str, &str)]) {
    let url = format!("{}{path}", world.gatehouse_url);
    let mut request = no_redirect_client().post(&url).form(form);
    if let Some(cookie) = &world.session_cookie {
        request = request.header(
            reqwest::header::COOKIE,
            format!("{SESSION_COOKIE}={cookie}"),
        );
    }
    let res = request.send().await.expect("form submission failed");
    world.record_response(res).await;
}

#[when(expr = "I submit the new user form for {string} with password {string}")]
async fn submit_new_user(world: &mut ForgeWorld, username: String, password: String) {
    post_form(
        world,
        "/ui/admin/users",
        &[
            ("username", &username),
            ("password", &password),
            ("role", "user"),
        ],
    )
    .await;
}

#[when(expr = "I submit the permission form giving {string} {string} on {string}")]
async fn submit_permissions(
    world: &mut ForgeWorld,
    username: String,
    level: String,
    service: String,
) {
    let field = format!("perm_{service}_{level}");
    post_form(
        world,
        &format!("/ui/admin/users/{username}"),
        &[("role", "user"), (&field, "on"), ("password", "")],
    )
    .await;
}

#[when(expr = "I submit the delete form for {string}")]
async fn submit_delete(world: &mut ForgeWorld, username: String) {
    post_form(world, &format!("/ui/admin/users/{username}/delete"), &[]).await;
}

#[when("I submit the form removing my own admin role")]
async fn submit_self_demote(world: &mut ForgeWorld) {
    let username = world.username.clone();
    post_form(
        world,
        &format!("/ui/admin/users/{username}"),
        &[("role", "user"), ("password", "")],
    )
    .await;
}

/// The pages redirect after every write, carrying the outcome. Asserting on the
/// `Location` is how a scenario reads it without following the redirect.
#[then(expr = "the redirect should report {string}")]
async fn redirect_reports(world: &mut ForgeWorld, marker: String) {
    let location = world
        .last_response_headers
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        location.contains(&marker),
        "redirect '{location}' does not carry '{marker}'"
    );
}

/// One control per service, named after it. Its absence would mean the matrix is
/// not being driven by `SERVICE_AUDIENCES`.
#[then(expr = "the page should offer an access control for {string}")]
async fn offers_control(world: &mut ForgeWorld, service: String) {
    let body = world.last_body.clone().unwrap_or_default();
    let field = format!("name=\"perm_{service}_");
    assert!(
        body.contains(&field),
        "no access control for '{service}' on the page"
    );
}

// ---------------------------------------------------------------------------
// Registration and password reset
//
// `LoggingSender` never puts the link anywhere but gatehouse's own stdout
// (`docker/gatehouse-service/src/email.rs`); `services::wait_for_gatehouse_log`
// is what lets these steps read it back the same way a developer would.
// ---------------------------------------------------------------------------

#[when(expr = "I register as {string} with password {string} and email {string}")]
async fn register(world: &mut ForgeWorld, username: String, password: String, email: String) {
    let url = format!("{}/ui/register", world.gatehouse_url);
    let res = no_redirect_client()
        .post(&url)
        .form(&[
            ("username", username.as_str()),
            ("password", password.as_str()),
            ("email", email.as_str()),
        ])
        .send()
        .await
        .expect("register request failed");
    world.record_response(res).await;
}

#[when(expr = "I follow the verification link emailed to {string}")]
async fn follow_verification_link(world: &mut ForgeWorld, email: String) {
    let marker = format!("email(verification) to={email}");
    let line = crate::services::wait_for_gatehouse_log(&marker)
        .await
        .unwrap_or_else(|| panic!("no verification email logged for {email}"));
    let link = extract_link(&line);
    let res = no_redirect_client()
        .get(&link)
        .send()
        .await
        .expect("verify request failed");
    world.record_response(res).await;
}

#[when(expr = "I request a password reset for {string}")]
async fn request_password_reset(world: &mut ForgeWorld, username: String) {
    let url = format!("{}/ui/forgot-password", world.gatehouse_url);
    let res = no_redirect_client()
        .post(&url)
        .form(&[("username", username.as_str())])
        .send()
        .await
        .expect("forgot-password request failed");
    world.record_response(res).await;
}

#[when(
    expr = "I follow the password reset link emailed to {string} and set the password to {string}"
)]
async fn follow_reset_link(world: &mut ForgeWorld, email: String, new_password: String) {
    let marker = format!("email(password-reset) to={email}");
    let line = crate::services::wait_for_gatehouse_log(&marker)
        .await
        .unwrap_or_else(|| panic!("no password-reset email logged for {email}"));
    let link = extract_link(&line);
    let token = link
        .split_once("token=")
        .expect("reset link had no token")
        .1;
    let url = format!("{}/ui/reset-password", world.gatehouse_url);
    let res = no_redirect_client()
        .post(&url)
        .form(&[("token", token), ("password", new_password.as_str())])
        .send()
        .await
        .expect("reset-password request failed");
    world.record_response(res).await;
}

/// Pulls the URL out of a `LoggingSender` line: `"...: visit {link} to ..."`.
fn extract_link(line: &str) -> String {
    let after_visit = line
        .split_once("visit ")
        .expect("log line had no 'visit <link>'")
        .1;
    after_visit
        .split(" to ")
        .next()
        .expect("link had no terminator")
        .to_string()
}
