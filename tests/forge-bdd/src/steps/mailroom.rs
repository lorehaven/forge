//! Steps for Mailroom: a person's mailboxes at a provider (the fake IMAP/SMTP of `mail.rs`), reading, sending, the panel, and
//! the estate's own mail.
//!
//! Mailroom keeps its state in Postgres, so these scenarios run only when `MAILROOM_BDD_DATABASE_URL` names one. Each scenario
//! uses a person of its own, so what one leaves behind does not reach the next.

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
    let url = format!("{}{path}", world.mailroom_url);
    let client = no_redirect_client();
    let mut request = match method {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        other => panic!("method {other}"),
    };
    if with_token && let Some(token) = &world.mailroom.token {
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
    world.mailroom.accounts.clear();
    world.mailroom.message = None;
}

#[given(expr = "I am not signed in to mailroom")]
fn not_signed_in(world: &mut ForgeWorld) {
    world.mailroom.token = None;
}

#[given(expr = "the provider mailbox holds a message {string} from {string}")]
fn provider_holds(_world: &mut ForgeWorld, subject: String, from: String) {
    let slug = subject.replace(' ', "-");
    let raw = format!(
        "From: {from}\r\nTo: {}\r\nSubject: {subject}\r\nDate: Tue, 29 Sep 2026 12:00:00 +0000\r\nMessage-ID: <{slug}@provider.example>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nBody of {subject}\r\n",
        mail::MAILBOX_USER
    );
    mail::infra()
        .imap
        .deliver(mail::MAILBOX_USER, "INBOX", raw.as_bytes(), &[]);
}

#[given(expr = "the provider mailbox holds an HTML message {string} with an attachment")]
fn provider_holds_html(_world: &mut ForgeWorld, subject: String) {
    let slug = subject.replace(' ', "-");
    let raw = format!(
        "From: Newsletter <news@provider.example>\r\nTo: {}\r\nSubject: {subject}\r\nDate: Tue, 29 Sep 2026 12:00:00 +0000\r\nMessage-ID: <{slug}@provider.example>\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"m\"\r\n\r\n--m\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p onclick=\"steal()\">Hello <b>reader</b></p><script>alert(1)</script><img src=\"https://tracker.example/p.gif\"><a href=\"javascript:alert(2)\">bad</a>\r\n\
--m\r\nContent-Type: application/pdf; name=\"report.pdf\"\r\nContent-Disposition: attachment; filename=\"report.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n--m--\r\n",
        mail::MAILBOX_USER
    );
    mail::infra()
        .imap
        .deliver(mail::MAILBOX_USER, "INBOX", raw.as_bytes(), &[]);
}

fn account_body(label: &str, password: &str, host: &str) -> Value {
    json!({
        "label": label,
        "address": mail::MAILBOX_USER,
        "username": mail::MAILBOX_USER,
        "password": password,
        "imap_host": host,
        "imap_port": mail::IMAP_PORT,
        "imap_security": "tls",
        "smtp_host": host,
        "smtp_port": mail::SMTP_PORT,
        "smtp_security": "tls",
        "signature": "",
        "tls_roots_pem": mail::certificate(),
        "save_sent": true,
    })
}

#[when(expr = "I add my provider mailbox as {string}")]
async fn add_mailbox(world: &mut ForgeWorld, label: String) {
    call(
        world,
        "POST",
        "/api/v1/accounts",
        Some(account_body(&label, mail::MAILBOX_PASSWORD, "localhost")),
        true,
    )
    .await;
    if let Some(id) = last_json(world)
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
    {
        world.mailroom.accounts.insert(label, id);
    }
}

#[when(expr = "I add a mailbox {string} with the password {string}")]
async fn add_mailbox_with_password(world: &mut ForgeWorld, label: String, password: String) {
    call(
        world,
        "POST",
        "/api/v1/accounts",
        Some(account_body(&label, &password, "localhost")),
        true,
    )
    .await;
}

#[when(expr = "I add a mailbox {string} on the server {string}")]
async fn add_mailbox_on(world: &mut ForgeWorld, label: String, host: String) {
    call(
        world,
        "POST",
        "/api/v1/accounts",
        Some(account_body(&label, mail::MAILBOX_PASSWORD, &host)),
        true,
    )
    .await;
}

#[when(expr = "I test the settings of a mailbox with the password {string}")]
async fn test_mailbox(world: &mut ForgeWorld, password: String) {
    call(
        world,
        "POST",
        "/api/v1/account-tests",
        Some(account_body("Test", &password, "localhost")),
        true,
    )
    .await;
}

#[when("I list my mailboxes")]
async fn list_mailboxes(world: &mut ForgeWorld) {
    call(world, "GET", "/api/v1/accounts", None, true).await;
}

#[when(expr = "I remove the mailbox {string}")]
async fn remove_mailbox(world: &mut ForgeWorld, label: String) {
    let id = world
        .mailroom
        .accounts
        .get(&label)
        .cloned()
        .unwrap_or_else(|| panic!("no mailbox {label}"));
    call(
        world,
        "DELETE",
        &format!("/api/v1/accounts/{id}"),
        None,
        true,
    )
    .await;
}

#[when(expr = "I refresh the mailbox {string}")]
async fn refresh(world: &mut ForgeWorld, label: String) {
    let id = world
        .mailroom
        .accounts
        .get(&label)
        .cloned()
        .unwrap_or_else(|| panic!("no mailbox {label}"));
    call(
        world,
        "POST",
        &format!("/api/v1/accounts/{id}/refresh"),
        None,
        true,
    )
    .await;
}

#[when("I list my inbox")]
async fn list_inbox(world: &mut ForgeWorld) {
    call(
        world,
        "GET",
        "/api/v1/messages?role=inbox&limit=100",
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
        &format!("/api/v1/messages?role=inbox&q={}", text.replace(' ', "%20")),
        None,
        true,
    )
    .await;
}

/// The id of the message with this subject in the person's inbox.
async fn message_id(world: &mut ForgeWorld, subject: &str) -> String {
    call(
        world,
        "GET",
        "/api/v1/messages?role=inbox&limit=100",
        None,
        true,
    )
    .await;
    last_json(world)
        .get("items")
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items
                .iter()
                .find(|m| m.get("subject").and_then(|s| s.as_str()) == Some(subject))
        })
        .and_then(|m| m.get("id").and_then(|i| i.as_str()))
        .unwrap_or_else(|| panic!("no message {subject:?} in the inbox"))
        .to_string()
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

#[when(expr = "I remember the mailbox {string}")]
fn remember_mailbox(world: &mut ForgeWorld, label: String) {
    world.mailroom.remembered = world.mailroom.accounts.get(&label).cloned();
}

#[when("I try to send from the remembered mailbox")]
async fn send_from_remembered(world: &mut ForgeWorld) {
    let id = world
        .mailroom
        .remembered
        .clone()
        .expect("remember a mailbox first");
    call(world, "POST", "/api/v1/outgoing", Some(json!({"account_id": id, "to": ["friend@example.org"], "subject": "Spoof", "text": "x"})), true).await;
}

#[when("I mark the opened message as read")]
async fn mark_read(world: &mut ForgeWorld) {
    let id = opened(world);
    call(
        world,
        "POST",
        "/api/v1/message-changes",
        Some(json!({"ids": [id], "seen": true})),
        true,
    )
    .await;
}

#[when("I move the opened message to the trash")]
async fn move_to_trash(world: &mut ForgeWorld) {
    let id = opened(world);
    call(
        world,
        "POST",
        "/api/v1/message-changes",
        Some(json!({"ids": [id], "move_to": "trash"})),
        true,
    )
    .await;
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

#[when(
    expr = "I send a message to {string} from the mailbox {string} whose subject has a line break"
)]
async fn send_with_break(world: &mut ForgeWorld, to: String, label: String) {
    let id = world
        .mailroom
        .accounts
        .get(&label)
        .cloned()
        .unwrap_or_else(|| panic!("no mailbox {label}"));
    call(
        world,
        "POST",
        "/api/v1/outgoing",
        Some(json!({"account_id": id, "to": [to], "subject": "Hello\r\nBcc: evil@example.net", "text": "x"})),
        true,
    )
    .await;
}

#[when(
    expr = "I send a message to {string} and {string} from the mailbox {string} with a line break in an address"
)]
async fn send_with_address_break(
    world: &mut ForgeWorld,
    first: String,
    second: String,
    label: String,
) {
    let id = world
        .mailroom
        .accounts
        .get(&label)
        .cloned()
        .unwrap_or_else(|| panic!("no mailbox {label}"));
    call(
        world,
        "POST",
        "/api/v1/outgoing",
        Some(json!({"account_id": id, "to": [first], "cc": [format!("{second}\r\nBcc: evil@example.net")], "subject": "Hello", "text": "x"})),
        true,
    )
    .await;
}

#[when(expr = "I download attachment {int} of the opened message")]
async fn download(world: &mut ForgeWorld, index: u32) {
    let id = opened(world);
    call(
        world,
        "GET",
        &format!("/api/v1/messages/{id}/attachments/{index}"),
        None,
        true,
    )
    .await;
}

#[when(expr = "I send {string} to {string} from the mailbox {string}")]
async fn send_from(world: &mut ForgeWorld, subject: String, to: String, label: String) {
    let id = world
        .mailroom
        .accounts
        .get(&label)
        .cloned()
        .unwrap_or_else(|| panic!("no mailbox {label}"));
    call(
        world,
        "POST",
        "/api/v1/outgoing",
        Some(json!({"account_id": id, "to": [to], "subject": subject, "text": format!("Text of {subject}")})),
        true,
    )
    .await;
}

#[when(expr = "I reply {string} to the opened message from the mailbox {string}")]
async fn reply_to_opened(world: &mut ForgeWorld, subject: String, label: String) {
    let id = world
        .mailroom
        .accounts
        .get(&label)
        .cloned()
        .unwrap_or_else(|| panic!("no mailbox {label}"));
    let message = opened(world);
    call(
        world,
        "POST",
        "/api/v1/outgoing",
        Some(json!({"account_id": id, "to": ["news@provider.example"], "subject": subject, "text": "Thanks", "in_reply_to": message})),
        true,
    )
    .await;
}

#[when(expr = "I add the header {string} to a message to {string}")]
async fn inject_header(world: &mut ForgeWorld, subject: String, to: String) {
    let id = world
        .mailroom
        .accounts
        .values()
        .next()
        .cloned()
        .expect("add a mailbox first");
    call(
        world,
        "POST",
        "/api/v1/outgoing",
        Some(json!({"account_id": id, "to": [to], "subject": subject, "text": "x"})),
        true,
    )
    .await;
}

// -- the estate's own mail ------------------------------------------------

async fn service_token(world: &ForgeWorld, scope: &str) -> String {
    crate::world::mint_test_token(
        &world.client,
        &world.gatehouse_url,
        "bdd-service",
        &["mailroom"],
        scope,
    )
    .await
}

#[when(expr = "a service sends {string} to {string} through mailroom")]
async fn service_sends(world: &mut ForgeWorld, subject: String, to: String) {
    let token = service_token(world, "service").await;
    let body = json!({"to": [to], "subject": subject, "text": format!("Text of {subject}"), "idempotency_key": format!("bdd-{subject}")});
    world.mailroom.token = Some(token);
    call(world, "POST", "/api/v1/send", Some(body), true).await;
}

#[when(expr = "a service sends {string} to {string} as {string} through mailroom")]
async fn service_sends_as(world: &mut ForgeWorld, subject: String, to: String, from: String) {
    let token = service_token(world, "service").await;
    world.mailroom.token = Some(token);
    call(
        world,
        "POST",
        "/api/v1/send",
        Some(json!({"from": from, "to": [to], "subject": subject, "text": "x"})),
        true,
    )
    .await;
}

#[when(expr = "I post to the estate send endpoint as {string} with {string}")]
async fn send_as(world: &mut ForgeWorld, user: String, scope: String) {
    let token = crate::world::mint_test_token(
        &world.client,
        &world.gatehouse_url,
        &user,
        &["mailroom"],
        &scope,
    )
    .await;
    world.mailroom.token = Some(token);
    call(
        world,
        "POST",
        "/api/v1/send",
        Some(json!({"to": ["x@example.org"], "subject": "no", "text": "no"})),
        true,
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

#[when(expr = "I allow the sender address {string}")]
async fn allow_sender(world: &mut ForgeWorld, address: String) {
    call(
        world,
        "PUT",
        "/api/v1/admin/settings",
        Some(json!({"identities": [
            {"address": "noreply@bdd.example", "name": "Forge", "default": true},
            {"address": address, "name": "", "default": false}
        ]})),
        true,
    )
    .await;
}

// -- what the fake servers saw -----------------------------------------

#[then(expr = "the sending server should receive a message to {string} with the subject {string}")]
async fn server_receives(_world: &mut ForgeWorld, to: String, subject: String) {
    let message = mail::wait_for(&to, None)
        .await
        .unwrap_or_else(|| panic!("no mail reached the sending server for {to}"));
    let mut found = false;
    for _ in 0..50 {
        if mail::received().iter().any(|m| {
            m.to.iter().any(|t| t.eq_ignore_ascii_case(&to))
                && m.header("subject").as_deref() == Some(subject.as_str())
        }) {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        found,
        "no message to {to} with subject {subject:?}; the newest was {:?}",
        message.header("subject")
    );
}

fn newest(to: &str, subject: &str) -> mail_testkit::Received {
    mail::received()
        .into_iter()
        .rev()
        .find(|m| {
            m.to.iter().any(|t| t.eq_ignore_ascii_case(to))
                && m.header("subject").as_deref() == Some(subject)
        })
        .unwrap_or_else(|| panic!("no message to {to} with subject {subject:?}"))
}

#[then(expr = "the message to {string} with the subject {string} should be sent as {string}")]
async fn sent_as(_world: &mut ForgeWorld, to: String, subject: String, from: String) {
    server_receives(_world, to.clone(), subject.clone()).await;
    assert_eq!(newest(&to, &subject).from, from);
}

#[then(expr = "the message to {string} with the subject {string} should be logged in as {string}")]
async fn logged_in_as(_world: &mut ForgeWorld, to: String, subject: String, user: String) {
    server_receives(_world, to.clone(), subject.clone()).await;
    assert_eq!(
        newest(&to, &subject).auth_user.as_deref(),
        Some(user.as_str())
    );
}

#[then(expr = "the message to {string} with the subject {string} should have the header {string}")]
async fn has_header(_world: &mut ForgeWorld, to: String, subject: String, name: String) {
    server_receives(_world, to.clone(), subject.clone()).await;
    assert!(
        newest(&to, &subject).header(&name).is_some(),
        "no {name} header"
    );
}

#[then(expr = "the message to {string} with the subject {string} should contain {string}")]
async fn message_contains(_world: &mut ForgeWorld, to: String, subject: String, text: String) {
    server_receives(_world, to.clone(), subject.clone()).await;
    let parsed = mail_client::message::parse(&newest(&to, &subject).data);
    assert!(parsed.text.contains(&text), "text was {:?}", parsed.text);
}

#[then(expr = "the provider folder {string} should hold a message with the subject {string}")]
async fn provider_has(_world: &mut ForgeWorld, folder: String, subject: String) {
    for _ in 0..50 {
        let held = mail::infra().imap.messages(mail::MAILBOX_USER, &folder);
        if held
            .iter()
            .any(|m| String::from_utf8_lossy(&m.raw).contains(&format!("Subject: {subject}\r\n")))
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("{folder} holds no message with the subject {subject:?}");
}

#[then(expr = "the provider folder {string} should not hold a message with the subject {string}")]
async fn provider_lacks(_world: &mut ForgeWorld, folder: String, subject: String) {
    let held = mail::infra().imap.messages(mail::MAILBOX_USER, &folder);
    assert!(
        !held
            .iter()
            .any(|m| String::from_utf8_lossy(&m.raw).contains(&format!("Subject: {subject}\r\n"))),
        "{folder} holds {subject:?}"
    );
}

#[then(expr = "the provider message {string} should be flagged {string}")]
async fn provider_flag(_world: &mut ForgeWorld, subject: String, flag: String) {
    let found = mail::infra()
        .imap
        .messages(mail::MAILBOX_USER, "INBOX")
        .into_iter()
        .find(|m| String::from_utf8_lossy(&m.raw).contains(&format!("Subject: {subject}\r\n")))
        .unwrap_or_else(|| panic!("no provider message {subject}"));
    assert!(
        found.flags.iter().any(|f| f.eq_ignore_ascii_case(&flag)),
        "flags: {:?}",
        found.flags
    );
}

#[then(expr = "the message list should show {int} message(s)")]
async fn list_shows(world: &mut ForgeWorld, count: u64) {
    let total = last_json(world)
        .get("total")
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);
    assert_eq!(total, count, "{:?}", world.last_body);
}

#[then(expr = "the response should not contain the mailbox password")]
fn no_password(world: &mut ForgeWorld) {
    let body = world.last_body.clone().unwrap_or_default();
    assert!(
        !body.contains(mail::MAILBOX_PASSWORD),
        "the password is in the answer"
    );
    assert!(!body.contains("v1."), "a sealed secret is in the answer");
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

#[then(expr = "mailroom should have sent a {string} message to {string}")]
async fn estate_mail(_world: &mut ForgeWorld, kind: String, to: String) {
    let message = mail::wait_for(&to, Some(&kind))
        .await
        .unwrap_or_else(|| panic!("no {kind} mail reached the sending server for {to}"));
    assert_eq!(message.from, "noreply@bdd.example");
    assert_eq!(message.auth_user.as_deref(), Some(mail::RELAY_USER));
    assert_eq!(
        message.header("auto-submitted").as_deref(),
        Some("auto-generated")
    );
    assert_eq!(
        message.header("x-mailroom-source").as_deref(),
        Some("gatehouse-mailroom")
    );
}

#[then(expr = "the {string} message to {string} should link to gatehouse")]
async fn links_to_gatehouse(_world: &mut ForgeWorld, kind: String, to: String) {
    let message = mail::wait_for(&to, Some(&kind))
        .await
        .unwrap_or_else(|| panic!("no {kind} mail for {to}"));
    let link = mail::link_in(&message).expect("a link in the message");
    assert!(
        link.starts_with("http://127.0.0.1:5443/gatehouse/"),
        "the link points at {link}"
    );
}
