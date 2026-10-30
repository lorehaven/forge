#![allow(clippy::unwrap_used, clippy::too_many_lines, clippy::type_complexity)]

use std::cell::RefCell;
use std::rc::Rc;

use quench_core::future::block_on;
use sage_app::{Msg, Request};
use sage_core::SendBody;
use sage_web::client::{Boxed, Client, HttpRequest, HttpResponse, Transport, base_of};

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
        Box::pin(async move {
            if status == 0 {
                Err("connection refused".to_string())
            } else {
                Ok(HttpResponse { status, body })
            }
        })
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

const T: &str = "2026-10-29T10:00:00Z";

fn conversation_json(id: &str) -> String {
    format!(
        r#"{{"id":"{id}","title":"t","project_id":null,"messages":[{{"id":"m","parent_id":null,"role":"user","content":"hi","created_at":"{T}","siblings":["m"],"sources":[],"attachments":[]}}]}}"#
    )
}

fn file_json(id: &str, status: &str) -> String {
    format!(
        r#"{{"id":"{id}","owner":"me","file_name":"a.txt","mime_type":"text/plain","file_size":3,"conversation_id":"c1","project_id":null,"message_id":null,"status":"{status}","error_message":null,"created_at":"{T}","updated_at":"{T}"}}"#
    )
}

fn run(client: &Client<Standin>, request: Request) -> Vec<Msg> {
    block_on(client.execute(request))
}

#[quench_test::test]
fn the_prefix_is_read_from_the_pages_own_path() {
    assert_eq!(base_of("/sage/ui/home"), "/sage");
    assert_eq!(base_of("/ui/home"), "");
    assert_eq!(base_of("/a/b/ui/initializing"), "/a/b");
    assert_eq!(base_of("/sage/ui"), "/sage");
}

#[quench_test::test]
fn the_models_are_read_and_a_dead_service_is_a_failure() {
    let models = r#"{"switchboard_ok":true,"ready":true,"instances":[{"id":"i","model":"m"}],"defaults":[],"search_providers":[{"id":"duckduckgo","label":"DuckDuckGo"}],"default_search_provider":"duckduckgo","accept":".txt"}"#;
    let (c, seen, _) = client(move |_| (200, models.to_string()));
    let msgs = run(&c, Request::Models);
    assert!(matches!(&msgs[..], [Msg::Models(m)] if m.ready && m.instances.len() == 1));
    assert_eq!(seen.borrow()[0], ("GET".into(), "/api/v1/models".into()));

    let (c, _, _) = client(|_| (0, String::new()));
    let msgs = run(&c, Request::Models);
    assert!(
        matches!(&msgs[..], [Msg::Failed(why)] if why.contains("connection refused")),
        "{msgs:?}"
    );
}

#[quench_test::test]
fn statuses_become_messages_a_lost_session_a_refusal_a_failure() {
    let (c, _, _) = client(|_| (401, String::new()));
    assert_eq!(run(&c, Request::Models), vec![Msg::SignedOut]);
    assert_eq!(
        run(&c, Request::DeleteConversation("x".into())),
        vec![Msg::SignedOut]
    );
    let (c, _, _) = client(|_| (500, "boom".into()));
    let msgs = run(&c, Request::Models);
    assert!(
        matches!(&msgs[..], [Msg::Failed(why)] if why.contains("500")),
        "{msgs:?}"
    );
    let (c, _, _) = client(|_| (400, "api_error_empty_message".into()));
    let msgs = run(
        &c,
        Request::Send(SendBody {
            instance_id: "i".into(),
            message: "x".into(),
            conversation_id: "c".into(),
            project_id: None,
            search_provider: None,
            file_ids: String::new(),
        }),
    );
    assert_eq!(msgs, vec![Msg::Rejected("api_error_empty_message".into())]);
    // prose is not passed on as if it were a code
    let (c, _, _) = client(|_| (400, "some long <html> error page".into()));
    let msgs = run(
        &c,
        Request::CreateProject(sage_core::NewProject { name: "x".into() }),
    );
    assert_eq!(msgs, vec![Msg::Rejected("the service answered 400".into())]);
}

#[quench_test::test]
fn the_sidebar_reads_projects_then_conversations() {
    let (c, seen, _) = client(|r| match r.path.as_str() {
        "/api/v1/projects" => (
            200,
            format!(r#"[{{"id":"p","name":"Atlas","updated_at":"{T}"}}]"#),
        ),
        "/api/v1/conversations" => (
            200,
            format!(r#"[{{"id":"c","title":"t","project_id":"p","updated_at":"{T}"}}]"#),
        ),
        other => panic!("{other}"),
    });
    let msgs = run(&c, Request::Sidebar);
    assert!(matches!(&msgs[0], Msg::Projects(p) if p[0].name == "Atlas"));
    assert!(matches!(&msgs[1], Msg::Conversations(l) if l[0].project_id.as_deref() == Some("p")));
    assert_eq!(seen.borrow().len(), 2);
}

#[quench_test::test]
fn a_missing_conversation_is_a_new_one_and_a_found_one_is_shown() {
    let (c, seen, _) = client(|r| {
        if r.path.ends_with("/gone") {
            (404, "api_error_conversation_not_found".into())
        } else {
            (200, conversation_json("c1"))
        }
    });
    let msgs = run(&c, Request::Conversation("c1".into()));
    assert!(matches!(&msgs[..], [Msg::Conversation(v)] if v.id == "c1" && v.messages.len() == 1));
    assert_eq!(seen.borrow()[0].1, "/api/v1/conversations/c1");
    assert_eq!(
        run(&c, Request::Conversation("gone".into())),
        vec![Msg::NoConversation("gone".into())]
    );
}

#[quench_test::test]
fn a_new_id_and_the_project_files() {
    let (c, _, _) = client(|r| {
        if r.path == "/api/v1/conversation-id" {
            (200, r#"{"id":"abc"}"#.into())
        } else if r.path.contains("p-foreign") {
            (403, String::new())
        } else {
            (200, format!("[{}]", file_json("f1", "ready")))
        }
    });
    assert_eq!(run(&c, Request::NewId), vec![Msg::NewId("abc".into())]);
    let msgs = run(&c, Request::ProjectFiles("p1".into()));
    assert!(
        matches!(&msgs[..], [Msg::ProjectFiles { project_id, files }] if project_id == "p1" && files.len() == 1)
    );
    // a project that is not the person's has no files to show
    let msgs = run(&c, Request::ProjectFiles("p-foreign".into()));
    assert!(matches!(&msgs[..], [Msg::ProjectFiles { files, .. }] if files.is_empty()));
}

#[quench_test::test]
fn sending_posts_the_body_and_starts_the_turn() {
    let (c, seen, bodies) = client(|_| (200, r#"{"id":"t1"}"#.into()));
    let body = SendBody {
        instance_id: "i1".into(),
        message: "hello".into(),
        conversation_id: "c1".into(),
        project_id: Some("p1".into()),
        search_provider: Some("brave".into()),
        file_ids: "f1,f2".into(),
    };
    let msgs = run(&c, Request::Send(body));
    assert_eq!(msgs, vec![Msg::TurnStarted("t1".into())]);
    assert_eq!(seen.borrow()[0], ("POST".into(), "/api/v1/turns".into()));
    let sent = bodies.borrow()[0].clone().unwrap();
    for needle in [
        r#""message":"hello""#,
        r#""conversation_id":"c1""#,
        r#""file_ids":"f1,f2""#,
        r#""project_id":"p1""#,
    ] {
        assert!(sent.contains(needle), "{needle}: {sent}");
    }
}

#[quench_test::test]
fn regenerate_and_resume_start_turns_and_resume_may_have_nothing_to_do() {
    let (c, seen, bodies) = client(|r| {
        if r.path.contains("resume") {
            if r.path.contains("/none/") {
                (200, r#"{"id":null}"#.into())
            } else {
                (200, r#"{"id":"t9"}"#.into())
            }
        } else {
            (200, r#"{"id":"t2"}"#.into())
        }
    });
    assert_eq!(
        run(&c, Request::Regenerate("a1".into())),
        vec![Msg::TurnStarted("t2".into())]
    );
    assert_eq!(seen.borrow()[0].1, "/api/v1/turns/regenerate");
    assert_eq!(
        bodies.borrow()[0].as_deref(),
        Some(r#"{"message_id":"a1"}"#)
    );
    assert_eq!(
        run(&c, Request::Resume("c1".into())),
        vec![Msg::TurnStarted("t9".into())]
    );
    assert_eq!(run(&c, Request::Resume("none".into())), vec![]);
}

#[quench_test::test]
fn an_edit_posts_reads_the_conversation_and_has_it_answered() {
    let (c, seen, bodies) = client(|r| match (r.method, r.path.as_str()) {
        ("POST", "/api/v1/messages/m1/edit") => (200, r#"{"id":"m2"}"#.into()),
        ("GET", "/api/v1/conversations/c1") => (200, conversation_json("c1")),
        ("POST", "/api/v1/conversations/c1/resume") => (200, r#"{"id":"t5"}"#.into()),
        other => panic!("{other:?}"),
    });
    let msgs = run(
        &c,
        Request::Edit {
            conversation_id: "c1".into(),
            message_id: "m1".into(),
            text: "better".into(),
        },
    );
    assert!(matches!(&msgs[0], Msg::Conversation(_)));
    assert_eq!(msgs[1], Msg::TurnStarted("t5".into()));
    assert_eq!(
        bodies.borrow()[0].as_deref(),
        Some(r#"{"new_content":"better"}"#)
    );
    assert_eq!(seen.borrow().len(), 3);

    // an edit the service refuses shows why and reads nothing more
    let (c, seen, _) = client(|_| (400, "api_error_edit_non_user".into()));
    let msgs = run(
        &c,
        Request::Edit {
            conversation_id: "c1".into(),
            message_id: "a".into(),
            text: "x".into(),
        },
    );
    assert_eq!(msgs, vec![Msg::Rejected("api_error_edit_non_user".into())]);
    assert_eq!(seen.borrow().len(), 1);
}

#[quench_test::test]
fn switching_posts_the_target_then_reads_the_conversation() {
    let (c, seen, bodies) = client(|r| {
        if r.method == "POST" {
            (204, String::new())
        } else {
            (200, conversation_json("c1"))
        }
    });
    let msgs = run(
        &c,
        Request::Switch {
            conversation_id: "c1".into(),
            target: "a1".into(),
        },
    );
    assert!(matches!(&msgs[..], [Msg::Conversation(_)]));
    assert_eq!(seen.borrow()[0].1, "/api/v1/conversations/c1/switch");
    assert_eq!(
        bodies.borrow()[0].as_deref(),
        Some(r#"{"target_message_id":"a1"}"#)
    );
}

#[quench_test::test]
fn deleting_a_conversation_reports_it_and_reads_the_list_again() {
    let (c, seen, _) = client(|r| {
        if r.method == "DELETE" {
            (204, String::new())
        } else {
            (200, "[]".into())
        }
    });
    let msgs = run(&c, Request::DeleteConversation("c1".into()));
    assert_eq!(msgs[0], Msg::ConversationDeleted("c1".into()));
    assert!(matches!(&msgs[1], Msg::Conversations(l) if l.is_empty()));
    assert_eq!(
        seen.borrow()[0],
        ("DELETE".into(), "/api/v1/conversations/c1".into())
    );
    // one that is already gone is gone
    let (c, _, _) = client(|_| (404, String::new()));
    assert_eq!(
        run(&c, Request::DeleteConversation("c1".into()))[0],
        Msg::ConversationDeleted("c1".into())
    );
}

#[quench_test::test]
fn files_are_deleted_polled_and_reprocessed() {
    let (c, seen, _) = client(|r| match (r.method, r.path.as_str()) {
        ("DELETE", _) => (204, String::new()),
        ("GET", _) => (200, file_json("f1", "processing")),
        ("POST", _) => (202, file_json("f1", "failed")),
        _ => unreachable!(),
    });
    assert_eq!(
        run(&c, Request::DeleteFile("f1".into())),
        vec![Msg::FileGone("f1".into())]
    );
    let msgs = run(&c, Request::FileStatus("f1".into()));
    assert!(matches!(&msgs[..], [Msg::File(f)] if f.status == "processing"));
    // a retry shows the file as processing, whatever state the service answered with
    let msgs = run(&c, Request::Reprocess("f1".into()));
    assert!(
        matches!(&msgs[..], [Msg::File(f)] if f.status == "processing" && f.error_message.is_none())
    );
    assert_eq!(
        seen.borrow()[2],
        ("POST".into(), "/api/v1/files/f1/reprocess".into())
    );

    // a file that vanished is gone from the page; an already running retry reads where it stands
    let (c, _, _) = client(|_| (404, String::new()));
    assert_eq!(
        run(&c, Request::FileStatus("f1".into())),
        vec![Msg::FileGone("f1".into())]
    );
    let (c, _, _) = client(|r| {
        if r.method == "POST" {
            (409, "api_error_file_already_processing".into())
        } else {
            (200, file_json("f1", "processing"))
        }
    });
    assert!(matches!(
        &run(&c, Request::Reprocess("f1".into()))[..],
        [Msg::File(_)]
    ));
}

#[quench_test::test]
fn a_project_is_created_and_a_bad_name_is_refused() {
    let (c, seen, bodies) = client(|_| {
        (
            201,
            format!(r#"{{"id":"p9","name":"Notes","updated_at":"{T}"}}"#),
        )
    });
    let msgs = run(
        &c,
        Request::CreateProject(sage_core::NewProject {
            name: "Notes".into(),
        }),
    );
    assert!(matches!(&msgs[..], [Msg::ProjectCreated(p)] if p.id == "p9"));
    assert_eq!(seen.borrow()[0], ("POST".into(), "/api/v1/projects".into()));
    assert_eq!(bodies.borrow()[0].as_deref(), Some(r#"{"name":"Notes"}"#));
    let (c, _, _) = client(|_| (400, "api_error_project_name".into()));
    let msgs = run(
        &c,
        Request::CreateProject(sage_core::NewProject { name: " ".into() }),
    );
    assert_eq!(msgs, vec![Msg::Rejected("api_error_project_name".into())]);
}

#[quench_test::test]
fn the_language_and_stopping_a_stream_are_the_browsers_business() {
    let (c, seen, _) = client(|_| panic!("nothing is sent"));
    assert!(run(&c, Request::Language("pl-PL".into())).is_empty());
    assert!(run(&c, Request::StopStream).is_empty());
    assert!(seen.borrow().is_empty());
}

quench_test::main!();
