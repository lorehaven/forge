//! Steps for Mailroom: people writing to each other, reading, the panel, and the estate's own mail.
//!
//! Mailroom keeps its state in Postgres, so these scenarios run only when `MAILROOM_BDD_DATABASE_URL` names one. Each scenario
//! uses people of its own, so what one leaves behind does not reach the next.

#![allow(
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    clippy::unused_async,
    clippy::used_underscore_binding,
    clippy::redundant_closure_for_method_calls,
    clippy::needless_pass_by_ref_mut
)]

use crate::mail;
use crate::world::{ForgeWorld, Target};
use quench_serde::{Value, json};
use quench_test_bdd::{given, then, when};

fn no_redirect_client() -> quench_client::transport::Client {
    quench_client::transport::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(quench_client::transport::Redirect::None)
        .build()
        .expect("http client")
}

/// One call to Mailroom's API with the scenario's token (or none).
async fn call(
    world: &mut ForgeWorld,
    method: &str,
    path: &str,
    body: Option<Value>,
    with_token: bool,
) {
    let token = if with_token {
        world.mailroom.token.clone()
    } else {
        None
    };
    call_with(world, method, path, body, token.as_deref()).await;
}

/// The same, presenting `token` (and leaving the scenario's own token alone).
async fn call_with(
    world: &mut ForgeWorld,
    method: &str,
    path: &str,
    body: Option<Value>,
    token: Option<&str>,
) {
    let url = format!("{}{path}", world.mailroom_url);
    let client = no_redirect_client();
    let mut request = match method {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        other => panic!("method {other}"),
    };
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .unwrap_or_else(|e| panic!("{method} {url}: {e}"));
    world.record_response(response).await;
}

fn last_json(world: &ForgeWorld) -> &Value {
    world.last_json.as_ref().unwrap_or_else(|| {
        panic!(
            "no JSON answer: {:?} {:?}",
            world.last_status, world.last_body
        )
    })
}

#[given("mailroom API is available")]
const fn available(world: &mut ForgeWorld) {
    world.target = Target::Mailroom;
}

#[given(expr = "I am signed in to mailroom as {string} with {string}")]
async fn signed_in(world: &mut ForgeWorld, user: String, scope: String) {
    let token = crate::world::mint_test_token(
        &world.client,
        &world.gatehouse_url,
        &user,
        &["mailroom"],
        &scope,
    )
    .await;
    world.mailroom.token = Some(token);
    world.mailroom.user = user;
    world.mailroom.message = None;
}

#[given(expr = "I am not signed in to mailroom")]
fn not_signed_in(world: &mut ForgeWorld) {
    world.mailroom.token = None;
}

/// The address of a user in the suite.
fn address(user: &str) -> String {
    format!("{user}@{}", mail::DOMAIN)
}

/// A token for another person, without changing who the scenario is.
async fn token_of(world: &ForgeWorld, user: &str, scope: &str) -> String {
    crate::world::mint_test_token(
        &world.client,
        &world.gatehouse_url,
        user,
        &["mailroom"],
        scope,
    )
    .await
}

// -- my mailbox ------------------------------------------------------------

#[when("I open my mailbox")]
async fn open_mailbox(world: &mut ForgeWorld) {
    call(world, "GET", "/api/v1/me", None, true).await;
}

#[when("I list my folders")]
async fn list_folders(world: &mut ForgeWorld) {
    call(world, "GET", "/api/v1/folders", None, true).await;
}

#[when(expr = "I look for people whose name starts with {string}")]
async fn people(world: &mut ForgeWorld, prefix: String) {
    call(
        world,
        "GET",
        &format!("/api/v1/people?q={prefix}"),
        None,
        true,
    )
    .await;
}

#[when(expr = "I list the messages in my {word}")]
async fn list_folder(world: &mut ForgeWorld, folder: String) {
    call(
        world,
        "GET",
        &format!("/api/v1/messages?folder={folder}&limit=100"),
        None,
        true,
    )
    .await;
}

#[when(expr = "I search my inbox for {string}")]
async fn search_inbox(world: &mut ForgeWorld, text: String) {
    call(
        world,
        "GET",
        &format!(
            "/api/v1/messages?folder=inbox&q={}",
            text.replace(' ', "%20")
        ),
        None,
        true,
    )
    .await;
}

// -- writing ------------------------------------------------------------------

#[when(expr = "I write {string} to {string}")]
async fn write(world: &mut ForgeWorld, subject: String, to: String) {
    call(
        world,
        "POST",
        "/api/v1/outgoing",
        Some(json!({"to": [to], "subject": subject, "text": format!("Text of {subject}")})),
        true,
    )
    .await;
}

#[when(expr = "I write {string} to {string} with a copy to {string}")]
async fn write_with_copy(world: &mut ForgeWorld, subject: String, to: String, cc: String) {
    call(
        world,
        "POST",
        "/api/v1/outgoing",
        Some(json!({"to": [to], "cc": [cc], "subject": subject, "text": format!("Text of {subject}")})),
        true,
    )
    .await;
}

#[when(expr = "I write {string} to {string} with a blind copy to {string}")]
async fn write_with_blind_copy(world: &mut ForgeWorld, subject: String, to: String, bcc: String) {
    call(
        world,
        "POST",
        "/api/v1/outgoing",
        Some(json!({"to": [to], "bcc": [bcc], "subject": subject, "text": format!("Text of {subject}")})),
        true,
    )
    .await;
}

#[when(expr = "I write to {string} with a line break in the subject")]
async fn write_with_break(world: &mut ForgeWorld, to: String) {
    call(
        world,
        "POST",
        "/api/v1/outgoing",
        Some(json!({"to": [to], "subject": "Hello\r\nBcc: evil@example.net", "text": "x"})),
        true,
    )
    .await;
}

#[when(expr = "I write to {string} with a line break in a recipient")]
async fn write_with_address_break(world: &mut ForgeWorld, to: String) {
    call(
        world,
        "POST",
        "/api/v1/outgoing",
        Some(json!({"to": [to.clone()], "cc": [format!("{to}\r\nBcc: evil@example.net")], "subject": "Hello", "text": "x"})),
        true,
    )
    .await;
}

#[when(expr = "I reply {string} to the opened message")]
async fn reply_to_opened(world: &mut ForgeWorld, subject: String) {
    let message = opened(world);
    let to = last_json(world)
        .pointer("/summary/from/address")
        .and_then(|v| v.as_str())
        .expect("an opened message")
        .to_string();
    call(
        world,
        "POST",
        "/api/v1/outgoing",
        Some(json!({"to": [to], "subject": subject, "text": "Thanks", "in_reply_to": message})),
        true,
    )
    .await;
}

// -- reading ------------------------------------------------------------------

/// The id of the message with this subject in one of the person's folders.
async fn message_id(world: &mut ForgeWorld, subject: &str) -> String {
    for folder in ["inbox", "sent", "trash"] {
        call(
            world,
            "GET",
            &format!("/api/v1/messages?folder={folder}&limit=100"),
            None,
            true,
        )
        .await;
        let found = last_json(world)
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|items| {
                items
                    .iter()
                    .find(|m| m.get("subject").and_then(|s| s.as_str()) == Some(subject))
            })
            .and_then(|m| m.get("id").and_then(|i| i.as_str()))
            .map(str::to_string);
        if let Some(id) = found {
            return id;
        }
    }
    panic!("no message {subject:?} in any folder");
}

#[when(expr = "I open the message {string}")]
async fn open_message(world: &mut ForgeWorld, subject: String) {
    let id = message_id(world, &subject).await;
    world.mailroom.message = Some(id.clone());
    call(world, "GET", &format!("/api/v1/messages/{id}"), None, true).await;
}

fn opened(world: &ForgeWorld) -> String {
    world
        .mailroom
        .message
        .clone()
        .expect("open a message first")
}

#[when("I remember the opened message")]
fn remember(world: &mut ForgeWorld) {
    world.mailroom.remembered = world.mailroom.message.clone();
}

#[when("I try to open the remembered message")]
async fn open_remembered(world: &mut ForgeWorld) {
    let id = world
        .mailroom
        .remembered
        .clone()
        .expect("remember a message first");
    call(world, "GET", &format!("/api/v1/messages/{id}"), None, true).await;
}

#[when("I try to open the html of the remembered message")]
async fn open_remembered_html(world: &mut ForgeWorld) {
    let id = world
        .mailroom
        .remembered
        .clone()
        .expect("remember a message first");
    call(
        world,
        "GET",
        &format!("/api/v1/messages/{id}/html"),
        None,
        true,
    )
    .await;
}

#[when("I try to move the remembered message to the trash")]
async fn move_remembered(world: &mut ForgeWorld) {
    let id = world
        .mailroom
        .remembered
        .clone()
        .expect("remember a message first");
    call(
        world,
        "POST",
        "/api/v1/message-changes",
        Some(json!({"ids": [id], "move_to": "trash"})),
        true,
    )
    .await;
}

async fn change(world: &mut ForgeWorld, change: Value) {
    let id = opened(world);
    let mut body = change;
    body["ids"] = json!([id]);
    call(world, "POST", "/api/v1/message-changes", Some(body), true).await;
}

#[when("I mark the opened message as read")]
async fn mark_read(world: &mut ForgeWorld) {
    change(world, json!({"seen": true})).await;
}

#[when("I star the opened message")]
async fn star(world: &mut ForgeWorld) {
    change(world, json!({"flagged": true})).await;
}

#[when("I move the opened message to the trash")]
async fn move_to_trash(world: &mut ForgeWorld) {
    change(world, json!({"move_to": "trash"})).await;
}

#[when("I move the opened message back to the inbox")]
async fn move_to_inbox(world: &mut ForgeWorld) {
    change(world, json!({"move_to": "inbox"})).await;
}

#[when("I delete the opened message for good")]
async fn delete_for_good(world: &mut ForgeWorld) {
    change(world, json!({"delete": true})).await;
}

#[when("I fetch the html of the opened message")]
async fn fetch_html(world: &mut ForgeWorld) {
    let id = opened(world);
    call(
        world,
        "GET",
        &format!("/api/v1/messages/{id}/html"),
        None,
        true,
    )
    .await;
}

#[when("I fetch the html of the opened message with the images allowed")]
async fn fetch_html_with_images(world: &mut ForgeWorld) {
    let id = opened(world);
    call(
        world,
        "GET",
        &format!("/api/v1/messages/{id}/html?remote=1"),
        None,
        true,
    )
    .await;
}

/// Follows nothing, so a redirect can be asserted on rather than chased.
#[when(expr = "I open the mailroom path {string}")]
async fn open_path(world: &mut ForgeWorld, path: String) {
    let url = format!("{}{path}", world.mailroom_url);
    let response = no_redirect_client()
        .get(&url)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url}: {e}"));
    world.record_response(response).await;
}

// -- other people, in the same scenario ----------------------------------------------

#[when(expr = "{string} signs in to mailroom with {string}")]
async fn another_signs_in(world: &mut ForgeWorld, user: String, scope: String) {
    let token = token_of(world, &user, &scope).await;
    world.mailroom.token = Some(token);
    world.mailroom.user = user;
    world.mailroom.message = None;
}

#[given(expr = "{string} has a mailbox")]
async fn has_mailbox(world: &mut ForgeWorld, user: String) {
    let token = token_of(world, &user, "mailroom:write").await;
    let url = format!("{}/api/v1/me", world.mailroom_url);
    let response = no_redirect_client()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url}: {e}"));
    assert_eq!(
        response.status().as_u16(),
        200,
        "{user} could not open a mailbox"
    );
}

// -- the estate's own mail ------------------------------------------------

async fn service_token(world: &ForgeWorld, scope: &str) -> String {
    token_of(world, "bdd-service", scope).await
}

#[when(expr = "a service sends {string} to {string} through mailroom")]
async fn service_sends(world: &mut ForgeWorld, subject: String, to: String) {
    let token = service_token(world, "service").await;
    let body = json!({"to": [to], "subject": subject, "text": format!("Text of {subject}"), "idempotency_key": format!("bdd-{subject}")});
    call_with(world, "POST", "/api/v1/send", Some(body), Some(&token)).await;
}

#[when(expr = "a service sends an html message {string} to {string} through mailroom")]
async fn service_sends_html(world: &mut ForgeWorld, subject: String, to: String) {
    let token = service_token(world, "service").await;
    call_with(
        world,
        "POST",
        "/api/v1/send",
        Some(json!({
            "to": [to],
            "subject": subject,
            "text": "Hello reader",
            "html": "<p onclick=\"steal()\">Hello <b>reader</b></p><script>alert(1)</script><img src=\"https://tracker.example/p.gif\"><a href=\"javascript:alert(2)\">bad</a>",
            "unsubscribe": "https://example.test/unsubscribe?t=1",
            "kind": "notification"
        })),
        Some(&token),
    )
    .await;
}

#[when(expr = "I post to the estate send endpoint as {string} with {string}")]
async fn send_as(world: &mut ForgeWorld, user: String, scope: String) {
    let token = token_of(world, &user, &scope).await;
    call_with(
        world,
        "POST",
        "/api/v1/send",
        Some(json!({"to": ["someone"], "subject": "no", "text": "no"})),
        Some(&token),
    )
    .await;
}

// -- the panel -----------------------------------------------------------

#[when(expr = "I {word} {string} on the mailroom panel")]
async fn panel_get(world: &mut ForgeWorld, verb: String, path: String) {
    let method = match verb.as_str() {
        "read" => "GET",
        "post" => "POST",
        "delete" => "DELETE",
        other => panic!("verb {other}"),
    };
    call(world, method, &path, None, true).await;
}

#[when(expr = "I set the mailroom daily budget to {int}")]
async fn set_budget(world: &mut ForgeWorld, budget: u32) {
    call(
        world,
        "PUT",
        "/api/v1/admin/settings",
        Some(json!({"daily_budget": budget})),
        true,
    )
    .await;
}

#[when(expr = "I set the mailroom mailbox size to {int} megabytes")]
async fn set_quota(world: &mut ForgeWorld, megabytes: u32) {
    call(
        world,
        "PUT",
        "/api/v1/admin/settings",
        Some(json!({"quota_mb": megabytes})),
        true,
    )
    .await;
}

// -- what the answers should say --------------------------------------------------

#[then(expr = "the message list should show {int} message(s)")]
async fn list_shows(world: &mut ForgeWorld, count: u64) {
    let total = last_json(world)
        .get("total")
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);
    assert_eq!(total, count, "{:?}", world.last_body);
}

#[then(expr = "the first message should be from {string}")]
fn first_from(world: &mut ForgeWorld, address: String) {
    let from = last_json(world)
        .pointer("/items/0/from/address")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    assert_eq!(from, address, "{:?}", world.last_body);
}

#[then("the first message should be marked as automatic")]
fn first_automatic(world: &mut ForgeWorld) {
    let automated = last_json(world)
        .pointer("/items/0/automated")
        .and_then(Value::as_bool);
    assert_eq!(automated, Some(true), "{:?}", world.last_body);
}

#[then(expr = "the answer should name the address {string}")]
fn names_address(world: &mut ForgeWorld, user: String) {
    let body = world.last_body.clone().unwrap_or_default();
    assert!(body.contains(&address(&user)), "{body}");
}

#[then(expr = "the last answer should have the header {string} with {string}")]
fn answer_header(world: &mut ForgeWorld, name: String, expected: String) {
    let value = world
        .last_response_headers
        .get(name.as_str())
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        value.contains(&expected),
        "{name}: {value:?} does not contain {expected:?}"
    );
}

#[then(expr = "the last answer should not have the header {string} with {string}")]
fn answer_header_absent(world: &mut ForgeWorld, name: String, forbidden: String) {
    let value = world
        .last_response_headers
        .get(name.as_str())
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        !value.contains(&forbidden),
        "{name}: {value:?} contains {forbidden:?}"
    );
}

// -- gatehouse's mail, as the estate sees it ----------------------------------

/// Asks the panel (as an administrator) what the services delivered.
async fn delivery_log(world: &ForgeWorld) -> Value {
    let token = token_of(world, "bdd-admin", "mailroom:admin").await;
    let url = format!("{}/api/v1/admin/deliveries?limit=200", world.mailroom_url);
    let response = no_redirect_client()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url}: {e}"));
    let text = response.text().await.unwrap_or_default();
    json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

#[then(expr = "mailroom should have delivered a {string} message to {string}")]
async fn estate_mail(world: &mut ForgeWorld, kind: String, user: String) {
    let wanted = address(&user);
    for _ in 0..50 {
        let log = delivery_log(world).await;
        let hit = log
            .get("items")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .any(|d| {
                d.get("kind").and_then(|v| v.as_str()) == Some(kind.as_str())
                    && d.get("source").and_then(|v| v.as_str()) == Some("gatehouse")
                    && d.get("status").and_then(|v| v.as_str()) == Some("delivered")
                    && d.get("recipients")
                        .and_then(|v| v.as_array())
                        .is_some_and(|r| r.iter().any(|x| x.as_str() == Some(wanted.as_str())))
            });
        if hit {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("no {kind} message was delivered to {wanted}");
}

#[then(expr = "mailroom should not have delivered a {string} message to {string}")]
async fn estate_mail_absent(world: &mut ForgeWorld, kind: String, user: String) {
    // Give a wrongly delivered message the time it would need to show up.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let wanted = address(&user);
    let log = delivery_log(world).await;
    let hit = log
        .get("items")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .any(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some(kind.as_str())
                && d.get("recipients")
                    .and_then(|v| v.as_array())
                    .is_some_and(|r| r.iter().any(|x| x.as_str() == Some(wanted.as_str())))
        });
    assert!(!hit, "a {kind} message was delivered to {wanted}");
}

#[then(expr = "{string} should find a message from gatehouse in their mailroom inbox")]
async fn user_finds_it(world: &mut ForgeWorld, user: String) {
    let token = token_of(world, &user, "mailroom:read").await;
    let url = format!(
        "{}/api/v1/messages?folder=inbox&limit=100",
        world.mailroom_url
    );
    let response = no_redirect_client()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url}: {e}"));
    let text = response.text().await.unwrap_or_default();
    let list: Value = json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
    let from = address("gatehouse");
    let found = list
        .get("items")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .any(|m| {
            m.pointer("/from/address").and_then(|v| v.as_str()) == Some(from.as_str())
                && m.get("automated").and_then(Value::as_bool) == Some(true)
        });
    assert!(found, "{user} has no message from {from}: {text}");
}
