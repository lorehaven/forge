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
use mailroom_core::{Change, Compose};
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

const FOLDERS: &str = r#"[{"id":"inbox","unread":1,"total":2},{"id":"sent","unread":0,"total":1},{"id":"trash","unread":0,"total":0}]"#;
const SUMMARY: &str = r#"{"id":"m1","folder":"inbox","subject":"Hi","from":{"name":"Alice","address":"alice@home.example"},"to":[],"date":1,"preview":"","seen":false,"flagged":false,"answered":false,"attachments":false,"size":10}"#;
const OK: (u16, String) = (200, String::new());

fn routes(request: &HttpRequest) -> (u16, String) {
    let body = match (request.method, request.path.split('?').next().unwrap()) {
        ("GET", "/api/v1/me") => r#"{"username":"ann","address":"ann@home.example","can_write":true,"admin":true}"#.to_string(),
        ("GET", "/api/v1/folders") => FOLDERS.to_string(),
        ("GET", "/api/v1/messages") => format!(r#"{{"items":[{SUMMARY}],"total":1,"offset":0}}"#),
        ("GET", "/api/v1/messages/m1") => format!(r#"{{"summary":{SUMMARY},"cc":[],"message_id":"<m1@x>","text":"hello","has_html":false,"remote_images":0,"attachments":[],"unsubscribe":""}}"#),
        ("GET", "/api/v1/people") => r#"[{"name":"","address":"bob@home.example"}]"#.to_string(),
        ("GET", "/api/v1/admin/stats") => r#"{"mailboxes":3,"messages":9,"bytes":2097152,"delivered_24h":3,"failed_24h":0,"budget_left":497}"#.to_string(),
        ("GET" | "PUT", "/api/v1/admin/settings") => r#"{"domain":"home.example","daily_budget":500,"hourly_per_source":120,"hourly_per_person":60,"quota_mb":500}"#.to_string(),
        ("GET", "/api/v1/admin/mailboxes") | ("GET", "/api/v1/admin/deliveries") | ("GET", "/api/v1/admin/audit") => r#"{"items":[],"total":0}"#.to_string(),
        ("GET", "/api/v1/drafts") => r#"[{"id":"d1","to":"bob","cc":"","bcc":"","subject":"Hi","text":"dear","in_reply_to":null,"updated":5}]"#.to_string(),
        ("GET", "/api/v1/drafts/d1") | ("PUT", "/api/v1/drafts/d1") | ("POST", "/api/v1/drafts") => r#"{"id":"d1","to":"bob","cc":"","bcc":"","subject":"Hi","text":"dear","in_reply_to":null,"updated":5}"#.to_string(),
        ("GET", "/api/v1/messages/m1/thread") => format!("[{SUMMARY}]"),
        ("GET", "/api/v1/mutes") => r#"["bob@home.example"]"#.to_string(),
        ("PUT", "/api/v1/signature") => r#"{"text":"Ann"}"#.to_string(),
        ("POST" | "DELETE" | "PUT", _) => return OK,
        _ => return (404, String::new()),
    };
    (200, body)
}

fn run(client: &Client<Standin>, request: Request) -> Vec<Msg> {
    block_on(client.execute(request))
}

fn paths(seen: &Seen) -> Vec<String> {
    seen.borrow()
        .iter()
        .map(|(m, p)| format!("{m} {p}"))
        .collect()
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
            folder: "inbox".into(),
            ..ListQuery::default()
        }),
        "/api/v1/messages?folder=inbox&offset=0&limit=50"
    );
    assert_eq!(
        list_path(&ListQuery {
            folder: "sent".into(),
            text: " a&b ".into(),
            unread: true,
            flagged: true,
            offset: 100,
        }),
        "/api/v1/messages?folder=sent&q=a%26b&unread=1&flagged=1&offset=100&limit=50"
    );
    // Without a folder the service takes the inbox.
    assert_eq!(
        list_path(&ListQuery::default()),
        "/api/v1/messages?offset=0&limit=50"
    );
}

#[quench_test::test]
fn startup_gives_who_and_the_folders() {
    let (c, seen, _) = client(routes);
    let msgs = run(&c, Request::Startup);
    assert!(
        matches!(&msgs[0], Msg::Me(me) if me.username == "ann" && me.admin && me.address == "ann@home.example")
    );
    assert!(
        matches!(&msgs[1], Msg::Folders(f) if f.len() == 3 && f[0].id == "inbox" && f[0].unread == 1)
    );
    assert_eq!(msgs.len(), 2);
    assert_eq!(paths(&seen), ["GET /api/v1/me", "GET /api/v1/folders"]);
}

#[quench_test::test]
fn startup_stops_at_a_signed_out_session_or_a_refusal() {
    let (c, seen, _) = client(|_| (401, String::new()));
    assert_eq!(run(&c, Request::Startup), [Msg::SignedOut]);
    assert_eq!(
        seen.borrow().len(),
        1,
        "nothing else is asked of a signed-out session"
    );
    let (c, _, _) = client(|_| (403, String::new()));
    assert!(
        matches!(&run(&c, Request::Startup)[..], [Msg::Failed(t)] if t.contains("ask for access"))
    );
}

#[quench_test::test]
fn reads_become_messages() {
    let (c, seen, _) = client(routes);
    assert!(matches!(&run(&c, Request::Folders)[0], Msg::Folders(f) if f[1].id == "sent"));
    assert!(
        matches!(&run(&c, Request::List(ListQuery { folder: "inbox".into(), ..ListQuery::default() }))[0], Msg::Messages(l) if l.total == 1)
    );
    assert!(
        matches!(&run(&c, Request::Open("m1".into()))[0], Msg::Message(m) if m.text == "hello")
    );
    assert!(
        paths(&seen).contains(&"GET /api/v1/messages?folder=inbox&offset=0&limit=50".to_string())
    );
}

#[quench_test::test]
fn a_missing_message_and_a_server_that_is_down_are_told_apart() {
    let (c, _, _) = client(|_| (404, String::new()));
    assert_eq!(run(&c, Request::Open("x".into())), [Msg::NotFound]);
    let (c, _, _) = client(|_| (500, "boom".into()));
    assert_eq!(
        run(&c, Request::Folders),
        [Msg::Failed("the service answered 500".into())]
    );
}

#[quench_test::test]
fn an_unreadable_answer_is_said_not_swallowed() {
    let (c, _, _) = client(|_| (200, "not json".into()));
    assert!(matches!(&run(&c, Request::Folders)[0], Msg::Failed(t) if t.contains("unreadable")));
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
            r#"{"error":"only what is in the trash is deleted for good"}"#.into(),
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
            "only what is in the trash is deleted for good".into()
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
fn sending_says_why_when_it_does_not_go() {
    let (c, seen, bodies) = client(routes);
    let compose = Compose {
        to: vec!["bob".into()],
        subject: "s".into(),
        text: "t".into(),
        ..Compose::default()
    };
    assert_eq!(run(&c, Request::Send(compose.clone())), [Msg::Sent]);
    assert_eq!(
        seen.borrow()[0],
        ("POST".to_string(), "/api/v1/outgoing".to_string())
    );
    assert!(bodies.borrow()[0].as_deref().unwrap().contains("bob"));
    let (c, _, _) = client(|_| {
        (
            400,
            r#"{"error":"not sent: bob@home.example has no mailbox here"}"#.into(),
        )
    });
    assert!(
        matches!(&run(&c, Request::Send(compose.clone()))[0], Msg::Rejected(t) if t.contains("no mailbox"))
    );
    let (c, _, _) = client(|_| {
        (
            429,
            r#"{"error":"too many requests, try again later"}"#.into(),
        )
    });
    assert!(
        matches!(&run(&c, Request::Send(compose))[0], Msg::Rejected(t) if t.contains("too many"))
    );
}

#[quench_test::test]
fn people_are_asked_for_and_a_failure_is_only_silence() {
    let (c, seen, _) = client(routes);
    assert!(
        matches!(&run(&c, Request::People("b o".into()))[0], Msg::People(p) if p[0].address == "bob@home.example")
    );
    assert_eq!(paths(&seen), ["GET /api/v1/people?q=b%20o"]);
    let (c, _, _) = client(|_| (500, String::new()));
    assert!(run(&c, Request::People("b".into())).is_empty());
    let (c, _, _) = client(|_| (404, String::new()));
    assert!(run(&c, Request::People("b".into())).is_empty());
    let (c, _, _) = client(|_| (401, String::new()));
    assert_eq!(run(&c, Request::People("b".into())), [Msg::SignedOut]);
}

#[quench_test::test]
fn an_upload_is_removed_quietly() {
    let (c, seen, _) = client(routes);
    assert!(run(&c, Request::RemoveUpload("u/1".into())).is_empty());
    assert_eq!(paths(&seen), ["DELETE /api/v1/uploads/u%2F1"]);
    let (c, _, _) = client(|_| (500, String::new()));
    assert!(run(&c, Request::RemoveUpload("u1".into())).is_empty());
}

#[quench_test::test]
fn the_panel_calls() {
    let (c, seen, _) = client(routes);
    assert!(
        matches!(&run(&c, Request::Stats)[0], Msg::Stats(s) if s.budget_left == 497 && s.mailboxes == 3)
    );
    assert!(
        matches!(&run(&c, Request::Settings)[0], Msg::Settings(s) if s.daily_budget == 500 && s.domain == "home.example")
    );
    assert!(matches!(
        &run(&c, Request::SaveSettings(SettingsInput::default()))[0],
        Msg::SettingsSaved(_)
    ));
    assert!(matches!(
        &run(&c, Request::Mailboxes { offset: 50 })[0],
        Msg::Mailboxes(_)
    ));
    assert!(matches!(
        &run(&c, Request::Deliveries { offset: 0 })[0],
        Msg::Deliveries(_)
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
                owner: "bob".into(),
                disabled: true
            }
        )[0],
        Msg::Mailboxes(_)
    ));
    assert!(matches!(
        &run(
            &c,
            Request::SetDisabled {
                owner: "bob".into(),
                disabled: false
            }
        )[0],
        Msg::Mailboxes(_)
    ));
    assert!(matches!(
        &run(&c, Request::RemoveMailbox("bob".into()))[0],
        Msg::Mailboxes(_)
    ));
    assert!(run(&c, Request::Language("pl-PL".into())).is_empty());
    let paths = paths(&seen);
    for expected in [
        "GET /api/v1/admin/mailboxes?offset=50&limit=50",
        "GET /api/v1/admin/deliveries?offset=0&limit=50",
        "PUT /api/v1/admin/settings",
        "POST /api/v1/admin/mailboxes/bob/disable",
        "POST /api/v1/admin/mailboxes/bob/enable",
        "DELETE /api/v1/admin/mailboxes/bob",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "{expected}: {paths:?}"
        );
    }
}

#[quench_test::test]
fn a_failed_panel_change_is_said_and_does_not_reload_the_list() {
    let (c, seen, _) = client(|_| (404, String::new()));
    assert!(matches!(
        &run(
            &c,
            Request::SetDisabled {
                owner: "x".into(),
                disabled: true
            }
        )[..],
        [Msg::Rejected(t)] if t.contains("gone")
    ));
    assert!(matches!(
        &run(&c, Request::RemoveMailbox("x".into()))[..],
        [Msg::Rejected(_)]
    ));
    assert_eq!(seen.borrow().len(), 2);
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
    assert_eq!(run(&c, Request::Stats), [Msg::SignedOut]);
    let (c, _, _) = client(|_| {
        (
            400,
            r#"{"error":"the daily budget must be from 1 to 100000"}"#.into(),
        )
    });
    assert!(
        matches!(&run(&c, Request::SaveSettings(SettingsInput::default()))[0], Msg::Rejected(t) if t.contains("budget"))
    );
}

#[quench_test::test]
fn an_offline_service_is_said() {
    struct Down;
    impl Transport for Down {
        fn send(&self, _: HttpRequest) -> Boxed<'_, Result<HttpResponse, String>> {
            Box::pin(async { Err("no network".to_string()) })
        }
    }
    let c = Client::new(Down);
    assert!(matches!(&run_down(&c, Request::Stats)[0], Msg::Failed(t) if t.contains("no network")));
    assert!(
        matches!(&run_down(&c, Request::Send(Compose::default()))[0], Msg::Failed(t) if t.contains("no network"))
    );
}

fn run_down(client: &Client<impl Transport>, request: Request) -> Vec<Msg> {
    block_on(client.execute(request))
}

quench_test::main!();

#[quench_test::test]
fn drafts_are_listed_opened_kept_and_deleted() {
    let (client, seen, bodies) = client(routes);
    let list = run(&client, Request::Drafts);
    assert!(matches!(&list[..], [Msg::Drafts(l)] if l.len() == 1 && l[0].id == "d1"));
    assert!(
        matches!(&run(&client, Request::OpenDraft("d1".into()))[..], [Msg::Draft(d)] if d.subject == "Hi")
    );
    let input = mailroom_core::DraftInput {
        to: "bob".into(),
        subject: "Hi".into(),
        text: "dear".into(),
        ..Default::default()
    };
    // No id yet: created. With an id: changed.
    assert!(
        matches!(&run(&client, Request::SaveDraft { id: None, input: input.clone() })[..], [Msg::DraftSaved(d)] if d.id == "d1")
    );
    assert!(matches!(
        &run(
            &client,
            Request::SaveDraft {
                id: Some("d1".into()),
                input
            }
        )[..],
        [Msg::DraftSaved(_)]
    ));
    assert_eq!(
        run(&client, Request::DeleteDraft("d1".into())),
        vec![Msg::DraftDeleted]
    );
    let calls = seen.borrow().clone();
    for want in [
        ("POST", "/api/v1/drafts"),
        ("PUT", "/api/v1/drafts/d1"),
        ("DELETE", "/api/v1/drafts/d1"),
    ] {
        assert!(
            calls.iter().any(|(m, p)| (m.as_str(), p.as_str()) == want),
            "{want:?} in {calls:?}"
        );
    }
    assert!(
        bodies
            .borrow()
            .iter()
            .flatten()
            .any(|b| b.contains("\"subject\":\"Hi\""))
    );
}

#[quench_test::test]
fn senders_are_muted_and_let_back_in() {
    let (client, seen, _) = client(routes);
    assert_eq!(
        run(&client, Request::Mutes),
        vec![Msg::Mutes(vec!["bob@home.example".into()])]
    );
    assert_eq!(
        run(&client, Request::Mute("bob@home.example".into())),
        vec![Msg::MuteChanged {
            address: "bob@home.example".into(),
            muted: true
        }]
    );
    assert_eq!(
        run(&client, Request::Unmute("bob@home.example".into())),
        vec![Msg::MuteChanged {
            address: "bob@home.example".into(),
            muted: false
        }]
    );
    let calls = seen.borrow().clone();
    assert!(
        calls
            .iter()
            .any(|(m, p)| m == "PUT" && p.starts_with("/api/v1/mutes/bob")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|(m, p)| m == "DELETE" && p.starts_with("/api/v1/mutes/bob")),
        "{calls:?}"
    );
}

#[quench_test::test]
fn the_conversation_of_a_message_is_asked_for_and_its_absence_is_not_an_error() {
    let (client, seen, _) = client(routes);
    assert!(
        matches!(&run(&client, Request::Thread("m1".into()))[..], [Msg::Thread(l)] if l.len() == 1)
    );
    assert!(
        seen.borrow()
            .iter()
            .any(|(_, p)| p == "/api/v1/messages/m1/thread")
    );
    // Not found, or broken: the message is shown alone and nothing is said.
    assert!(run(&client, Request::Thread("gone".into())).is_empty());
}

#[quench_test::test]
fn the_signature_is_saved_and_the_saved_text_comes_back() {
    let (client, seen, bodies) = client(routes);
    assert_eq!(
        run(&client, Request::SaveSignature("Ann".into())),
        vec![Msg::SignatureSaved("Ann".into())]
    );
    assert!(
        seen.borrow()
            .iter()
            .any(|(m, p)| m == "PUT" && p == "/api/v1/signature")
    );
    assert!(
        bodies
            .borrow()
            .iter()
            .flatten()
            .any(|b| b.contains("\"text\":\"Ann\""))
    );
}
