#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::pedantic,
    clippy::nursery
)]

use std::cell::RefCell;
use std::rc::Rc;

use mailroom_app::{ListQuery, Msg, Request};
use mailroom_core::admin::SettingsInput;
use mailroom_core::{AccountInput, Change, Compose};
use mailroom_web::client::{
    Boxed, Client, HttpRequest, HttpResponse, Transport, base_of, list_path, segment,
};
use quench_core::future::block_on;

type Handler = Box<dyn Fn(&HttpRequest) -> (u16, String)>;
type Seen = Rc<RefCell<Vec<(String, String)>>>;
type Bodies = Rc<RefCell<Vec<Option<String>>>>;

/// A service that answers from a function and remembers what it was asked.
struct Standin {
    seen: Seen,
    bodies: Bodies,
    handler: Handler,
}

impl Transport for Standin {
    fn send(&self, request: HttpRequest) -> Boxed<'_, Result<HttpResponse, String>> {
        let (status, body) = (self.handler)(&request);
        self.seen
            .borrow_mut()
            .push((request.method.to_string(), request.path.clone()));
        self.bodies.borrow_mut().push(request.body);
        Box::pin(async move { Ok(HttpResponse { status, body }) })
    }
}

fn client(
    handler: impl Fn(&HttpRequest) -> (u16, String) + 'static,
) -> (Client<Standin>, Seen, Bodies) {
    let seen: Seen = Rc::default();
    let bodies: Bodies = Rc::default();
    (
        Client::new(Standin {
            seen: seen.clone(),
            bodies: bodies.clone(),
            handler: Box::new(handler),
        }),
        seen,
        bodies,
    )
}

const ACCOUNT: &str = r#"{"id":"a1","label":"Personal","address":"me@example.com","username":"me@example.com","imap_host":"imap.example.com","imap_port":993,"imap_security":"tls","smtp_host":"smtp.example.com","smtp_port":465,"smtp_security":"tls","state":"ok","last_error":null,"last_sync":null,"unread":1,"signature":"","custom_certificate":false,"save_sent":true}"#;
const SUMMARY: &str = r#"{"id":"m1","account_id":"a1","folder_id":"f1","subject":"Hi","from":{"name":"Alice","address":"alice@x.example"},"to":[],"date":1,"preview":"","seen":false,"flagged":false,"answered":false,"attachments":false,"size":10}"#;
const OK: (u16, String) = (200, String::new());

fn routes(request: &HttpRequest) -> (u16, String) {
    let body = match (request.method, request.path.split('?').next().unwrap()) {
        ("GET", "/api/v1/me") => r#"{"username":"ann","can_write":true,"admin":true,"max_accounts":10}"#.to_string(),
        ("GET", "/api/v1/presets") => "[]".to_string(),
        ("GET", "/api/v1/accounts") => format!("[{ACCOUNT}]"),
        ("POST", "/api/v1/accounts") | ("PUT", "/api/v1/accounts/a1") => ACCOUNT.to_string(),
        ("GET", "/api/v1/accounts/a1/folders") => r#"[{"id":"f1","account_id":"a1","path":"INBOX","name":"INBOX","role":"inbox","unread":1,"total":2}]"#.to_string(),
        ("GET", "/api/v1/messages") => format!(r#"{{"items":[{SUMMARY}],"total":1,"offset":0}}"#),
        ("GET", "/api/v1/messages/m1") => format!(r#"{{"summary":{SUMMARY},"cc":[],"reply_to":[],"message_id":"<m1@x>","text":"hello","has_html":false,"remote_images":0,"attachments":[],"list_unsubscribe":""}}"#),
        ("POST", "/api/v1/account-tests") => r#"{"imap":{"ok":true,"error":""},"smtp":{"ok":false,"error":"nope"}}"#.to_string(),
        ("GET", "/api/v1/admin/stats") => r#"{"accounts":1,"accounts_failing":0,"messages":2,"queued":0,"sent_24h":3,"failed_24h":0,"budget_left":497}"#.to_string(),
        ("GET", "/api/v1/admin/settings") | ("PUT", "/api/v1/admin/settings") => r#"{"transport":null,"identities":[],"private_hosts":[],"extra_ports":[],"daily_budget":500,"hourly_per_source":120,"max_accounts_per_user":10,"sync_interval_secs":120}"#.to_string(),
        ("POST", "/api/v1/admin/transport/test") => r#"{"imap":{"ok":true,"error":""},"smtp":{"ok":true,"error":""}}"#.to_string(),
        ("GET", "/api/v1/admin/accounts") => r#"{"items":[],"total":0}"#.to_string(),
        ("GET", "/api/v1/admin/outbox") => r#"{"items":[],"total":0}"#.to_string(),
        ("GET", "/api/v1/admin/audit") => r#"{"items":[],"total":0}"#.to_string(),
        ("POST" | "DELETE", _) => return OK,
        _ => return (404, String::new()),
    };
    (200, body)
}

fn run(client: &Client<Standin>, request: Request) -> Vec<Msg> {
    block_on(client.execute(request))
}

#[quench_test::test]
fn the_prefix_is_read_from_the_pages_own_path() {
    assert_eq!(base_of("/mailroom/ui/home"), "/mailroom");
    assert_eq!(base_of("/ui/home"), "");
    assert_eq!(base_of("/mailroom/ui"), "/mailroom");
    assert_eq!(base_of("/a/b/ui/message/x"), "/a/b");
    assert_eq!(base_of("/elsewhere"), "");
}

#[quench_test::test]
fn values_in_a_path_cannot_break_out_of_it() {
    assert_eq!(segment("abc-1.2_3~"), "abc-1.2_3~");
    assert_eq!(
        segment("a b/../c?d=e&f#g"),
        "a%20b%2F..%2Fc%3Fd%3De%26f%23g"
    );
    assert_eq!(segment("zażółć"), "za%C5%BC%C3%B3%C5%82%C4%87");
    assert_eq!(segment(""), "");
}

#[quench_test::test]
fn a_list_is_asked_for_with_its_filters() {
    assert_eq!(
        list_path(&ListQuery {
            role: Some("inbox".into()),
            ..ListQuery::default()
        }),
        "/api/v1/messages?role=inbox&offset=0&limit=50"
    );
    assert_eq!(
        list_path(&ListQuery {
            folder: Some("f 1".into()),
            text: " a&b ".into(),
            unread: true,
            offset: 100,
            ..ListQuery::default()
        }),
        "/api/v1/messages?folder=f%201&q=a%26b&unread=1&offset=100&limit=50"
    );
}

#[quench_test::test]
fn startup_gives_who_the_providers_and_the_mailboxes() {
    let (c, seen, _) = client(routes);
    let msgs = run(&c, Request::Startup);
    assert!(matches!(&msgs[0], Msg::Me(me) if me.username == "ann" && me.admin));
    assert!(matches!(&msgs[1], Msg::Presets(p) if p.is_empty()));
    assert!(matches!(&msgs[2], Msg::Accounts(a) if a[0].id == "a1"));
    assert_eq!(seen.borrow().len(), 3);
}

#[quench_test::test]
fn startup_stops_at_a_signed_out_session_or_a_refusal() {
    let (c, _, _) = client(|_| (401, String::new()));
    assert_eq!(run(&c, Request::Startup), [Msg::SignedOut]);
    let (c, _, _) = client(|_| (403, String::new()));
    assert!(
        matches!(&run(&c, Request::Startup)[..], [Msg::Failed(t)] if t.contains("ask for access"))
    );
}

#[quench_test::test]
fn reads_become_messages() {
    let (c, seen, _) = client(routes);
    assert!(
        matches!(&run(&c, Request::Folders("a1".into()))[0], Msg::Folders { account, folders } if account == "a1" && folders[0].role == "inbox")
    );
    assert!(
        matches!(&run(&c, Request::List(ListQuery { role: Some("inbox".into()), ..ListQuery::default() }))[0], Msg::Messages(l) if l.total == 1)
    );
    assert!(
        matches!(&run(&c, Request::Open("m1".into()))[0], Msg::Message(m) if m.text == "hello")
    );
    assert!(
        seen.borrow()
            .iter()
            .any(|(m, p)| m == "GET" && p == "/api/v1/messages?role=inbox&offset=0&limit=50")
    );
}

#[quench_test::test]
fn a_missing_message_and_a_server_that_is_down_are_told_apart() {
    let (c, _, _) = client(|_| (404, String::new()));
    assert_eq!(run(&c, Request::Open("x".into())), [Msg::NotFound]);
    let (c, _, _) = client(|_| {
        (
            502,
            r#"{"error":"the mail server did not answer in time"}"#.into(),
        )
    });
    assert_eq!(
        run(&c, Request::Open("x".into())),
        [Msg::Failed("the mail server did not answer in time".into())]
    );
    let (c, _, _) = client(|_| (500, "boom".into()));
    assert_eq!(
        run(&c, Request::Accounts),
        [Msg::Failed("the service answered 500".into())]
    );
}

#[quench_test::test]
fn an_unreadable_answer_is_said_not_swallowed() {
    let (c, _, _) = client(|_| (200, "not json".into()));
    assert!(matches!(&run(&c, Request::Accounts)[0], Msg::Failed(t) if t.contains("unreadable")));
}

#[quench_test::test]
fn changes_say_whether_the_message_stayed() {
    let (c, seen, bodies) = client(routes);
    let read = Change {
        ids: vec!["m1".into()],
        seen: Some(true),
        ..Change::default()
    };
    assert_eq!(run(&c, Request::Change(read)), [Msg::Changed]);
    let moved = Change {
        ids: vec!["m1".into()],
        move_to: Some("trash".into()),
        ..Change::default()
    };
    assert_eq!(run(&c, Request::Change(moved)), [Msg::Moved]);
    let deleted = Change {
        ids: vec!["m1".into()],
        delete: true,
        ..Change::default()
    };
    assert_eq!(run(&c, Request::Change(deleted)), [Msg::Moved]);
    assert_eq!(
        seen.borrow()[0],
        ("POST".to_string(), "/api/v1/message-changes".to_string())
    );
    assert!(
        bodies.borrow()[0]
            .as_deref()
            .unwrap()
            .contains("\"seen\":true")
    );
}

#[quench_test::test]
fn a_refused_change_carries_the_services_own_words() {
    let (c, _, _) = client(|_| {
        (
            400,
            r#"{"error":"messages are deleted for good only from the trash"}"#.into(),
        )
    });
    let change = Change {
        ids: vec!["m1".into()],
        delete: true,
        ..Change::default()
    };
    assert_eq!(
        run(&c, Request::Change(change.clone())),
        [Msg::Rejected(
            "messages are deleted for good only from the trash".into()
        )]
    );
    let (c, _, _) = client(|_| (404, String::new()));
    assert!(
        matches!(&run(&c, Request::Change(change.clone()))[0], Msg::Rejected(t) if t.contains("gone"))
    );
    let (c, _, _) = client(|_| (401, String::new()));
    assert_eq!(run(&c, Request::Change(change.clone())), [Msg::SignedOut]);
    let (c, _, _) = client(|_| (503, "<html>".into()));
    assert!(matches!(&run(&c, Request::Change(change))[0], Msg::Rejected(t) if t.contains("503")));
}

#[quench_test::test]
fn sending_and_refreshing() {
    let (c, seen, bodies) = client(routes);
    let compose = Compose {
        account_id: "a1".into(),
        to: vec!["bob@example.org".into()],
        subject: "s".into(),
        text: "t".into(),
        ..Compose::default()
    };
    assert_eq!(run(&c, Request::Send(compose)), [Msg::Sent]);
    assert_eq!(
        seen.borrow()[0],
        ("POST".to_string(), "/api/v1/outgoing".to_string())
    );
    assert!(
        bodies.borrow()[0]
            .as_deref()
            .unwrap()
            .contains("bob@example.org")
    );
    assert_eq!(run(&c, Request::Refresh("a1".into())), [Msg::Changed]);
    assert_eq!(
        seen.borrow()[1],
        (
            "POST".to_string(),
            "/api/v1/accounts/a1/refresh".to_string()
        )
    );
    let (c, _, _) = client(|_| {
        (
            429,
            r#"{"error":"too many requests, try again later"}"#.into(),
        )
    });
    assert!(
        matches!(&run(&c, Request::Refresh("a1".into()))[0], Msg::Rejected(t) if t.contains("too many"))
    );
}

#[quench_test::test]
fn mailboxes_are_added_changed_tested_and_removed() {
    let (c, seen, _) = client(routes);
    let input = AccountInput {
        label: "Personal".into(),
        address: "me@example.com".into(),
        ..AccountInput::default()
    };
    assert!(
        matches!(&run(&c, Request::AddAccount(input.clone()))[0], Msg::AccountSaved(a) if a.id == "a1")
    );
    assert!(matches!(
        &run(
            &c,
            Request::UpdateAccount {
                id: "a1".into(),
                input: input.clone()
            }
        )[0],
        Msg::AccountSaved(_)
    ));
    assert!(
        matches!(&run(&c, Request::TestAccount(input))[0], Msg::Tested(t) if t.imap.ok && !t.smtp.ok)
    );
    assert_eq!(
        run(&c, Request::RemoveAccount("a1".into())),
        [Msg::AccountRemoved("a1".into())]
    );
    assert!(run(&c, Request::RemoveUpload("u/1".into())).is_empty());
    let paths: Vec<String> = seen
        .borrow()
        .iter()
        .map(|(m, p)| format!("{m} {p}"))
        .collect();
    for expected in [
        "POST /api/v1/accounts",
        "PUT /api/v1/accounts/a1",
        "POST /api/v1/account-tests",
        "DELETE /api/v1/accounts/a1",
        "DELETE /api/v1/uploads/u%2F1",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "{expected}: {paths:?}"
        );
    }
    let (c, _, _) = client(|_| {
        (
            502,
            r#"{"error":"the mail server refused the login: check the password"}"#.into(),
        )
    });
    assert!(
        matches!(&run(&c, Request::AddAccount(AccountInput::default()))[0], Msg::Rejected(t) if t.contains("check the password"))
    );
}

#[quench_test::test]
fn the_panel_calls() {
    let (c, seen, _) = client(routes);
    assert!(matches!(&run(&c, Request::Stats)[0], Msg::Stats(s) if s.budget_left == 497));
    assert!(matches!(&run(&c, Request::Settings)[0], Msg::Settings(s) if s.daily_budget == 500));
    assert!(matches!(
        &run(&c, Request::SaveSettings(SettingsInput::default()))[0],
        Msg::SettingsSaved(_)
    ));
    assert!(matches!(&run(&c, Request::TestTransport)[0], Msg::TransportTested(t) if t.smtp.ok));
    assert!(matches!(
        &run(&c, Request::AdminAccounts { offset: 50 })[0],
        Msg::AdminAccounts(_)
    ));
    assert!(matches!(
        &run(
            &c,
            Request::Outbox {
                status: "queued".into(),
                offset: 0
            }
        )[0],
        Msg::Outbox(_)
    ));
    assert!(matches!(
        &run(&c, Request::Audit { offset: 0 })[0],
        Msg::Audit(_)
    ));
    // Changes of a list ask for the list again.
    assert!(matches!(
        &run(
            &c,
            Request::SetDisabled {
                id: "a1".into(),
                disabled: true
            }
        )[0],
        Msg::AdminAccounts(_)
    ));
    assert!(matches!(
        &run(&c, Request::AdminRemoveAccount("a1".into()))[0],
        Msg::AdminAccounts(_)
    ));
    assert!(matches!(
        &run(&c, Request::Retry("o1".into()))[0],
        Msg::Outbox(_)
    ));
    assert!(matches!(
        &run(&c, Request::RemoveQueued("o1".into()))[0],
        Msg::Outbox(_)
    ));
    assert!(run(&c, Request::Language("pl-PL".into())).is_empty());
    let paths: Vec<String> = seen
        .borrow()
        .iter()
        .map(|(m, p)| format!("{m} {p}"))
        .collect();
    for expected in [
        "GET /api/v1/admin/accounts?offset=50&limit=50",
        "GET /api/v1/admin/outbox?status=queued&offset=0&limit=50",
        "POST /api/v1/admin/accounts/a1/disable",
        "POST /api/v1/admin/outbox/o1/retry",
        "DELETE /api/v1/admin/outbox/o1",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "{expected}: {paths:?}"
        );
    }
}

#[quench_test::test]
fn a_panel_the_person_may_not_use_is_said() {
    let (c, _, _) = client(|_| (403, String::new()));
    assert!(matches!(&run(&c, Request::Stats)[0], Msg::Failed(t) if t.contains("ask for access")));
    let (c, _, _) = client(|_| (403, r#"{"error":"not allowed"}"#.into()));
    assert!(
        matches!(&run(&c, Request::SaveSettings(SettingsInput::default()))[0], Msg::Rejected(t) if t == "not allowed")
    );
    let (c, _, _) = client(|_| (401, String::new()));
    assert_eq!(run(&c, Request::TestTransport), [Msg::SignedOut]);
}

quench_test::main!();
