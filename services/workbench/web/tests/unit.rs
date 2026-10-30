#![allow(clippy::unwrap_used, clippy::too_many_lines, clippy::type_complexity)]

use std::cell::RefCell;
use std::rc::Rc;

use quench_core::future::block_on;
use workbench_app::{Msg, Request};
use workbench_core::{CreateIssue, CreateProject, UpdateIssue};
use workbench_web::client::{
    Boxed, Client, HttpRequest, HttpResponse, Transport, base_of, segment,
};

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

const PROJECT: &str = r#"{"id":"p1","key":"WB","name":"Workbench","description":null,"created_at":"2026-10-29T10:00:00Z","updated_at":"2026-10-29T10:00:00Z"}"#;

fn issue_json(id: &str, seq: i32, status: &str) -> String {
    format!(
        r#"{{"id":"{id}","project_id":"p1","parent_id":null,"seq":{seq},"kind":"task","title":"T{seq}","description":null,"status":"{status}","priority":"medium","assignee":null,"reporter":"dev","estimate":null,"created_at":"2026-10-29T10:00:00Z","updated_at":"2026-10-29T10:00:00Z"}}"#
    )
}

fn routes(request: &HttpRequest) -> (u16, String) {
    let body = match (request.method, request.path.as_str()) {
        ("GET", "/api/v1/me") => r#"{"username":"ola"}"#.to_string(),
        ("GET", "/api/v1/users") => r#"[{"username":"ola","display_name":null}]"#.to_string(),
        ("GET", "/api/v1/projects") => format!("[{PROJECT}]"),
        ("GET", "/api/v1/projects/p1") => PROJECT.to_string(),
        ("GET", "/api/v1/projects/p1/issues") => {
            format!(
                "[{},{}]",
                issue_json("i1", 1, "todo"),
                issue_json("i2", 2, "done")
            )
        }
        ("GET" | "PUT", "/api/v1/issues/i1") | ("POST", "/api/v1/issues/i1/transition") => {
            issue_json("i1", 1, "todo")
        }
        ("GET", "/api/v1/issues?key=WB-2") => issue_json("i2", 2, "done"),
        ("GET", "/api/v1/issues/i1/comments") => "[]".to_string(),
        ("GET", "/api/v1/issues/i1/links") => {
            r#"{"blocks":[],"blocked_by":[],"relates_to":[]}"#.to_string()
        }
        ("POST", "/api/v1/issues/i1/links") => {
            r#"{"id":"l1","issue_id":"i1","linked_issue_id":"i2","kind":"blocks","created_at":"x"}"#
                .to_string()
        }
        ("POST", "/api/v1/issues/i1/comments") => {
            r#"{"id":"c1","issue_id":"i1","author":"ola","body":"hi","created_at":"x"}"#.to_string()
        }
        ("DELETE", "/api/v1/issue-links/l1") => String::new(),
        ("POST", "/api/v1/projects") => PROJECT.to_string(),
        ("POST", "/api/v1/projects/p1/issues") => issue_json("i3", 3, "todo"),
        _ => return (404, r#"{"error":"nothing here"}"#.to_string()),
    };
    (200, body)
}

fn paths(seen: &Seen) -> Vec<String> {
    seen.borrow()
        .iter()
        .map(|(m, p)| format!("{m} {p}"))
        .collect()
}

#[quench_test::test]
fn the_service_prefix_is_what_comes_before_ui() {
    assert_eq!(base_of("/workbench/ui/home"), "/workbench");
    assert_eq!(base_of("/workbench/ui/issues/x"), "/workbench");
    assert_eq!(base_of("/workbench/ui"), "/workbench");
    assert_eq!(base_of("/ui/home"), "");
    assert_eq!(segment("a b/c"), "a%20b%2Fc");
}

#[quench_test::test]
fn startup_asks_who_and_the_projects_are_asked_for_apart() {
    let (c, seen, _) = client(routes);
    let msgs = block_on(c.execute(Request::Startup));
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0], Msg::Me("ola".into()));
    assert!(matches!(&msgs[1], Msg::Users(u) if u.len() == 1));
    assert_eq!(paths(&seen), ["GET /api/v1/me", "GET /api/v1/users"]);
    let msgs = block_on(c.execute(Request::Projects));
    assert!(matches!(&msgs[0], Msg::Projects(p) if p[0].key == "WB"));
}

#[quench_test::test]
fn a_board_is_a_project_and_its_issues() {
    let (c, seen, _) = client(routes);
    let msgs = block_on(c.execute(Request::Board("p1".into())));
    let Msg::Board(board) = &msgs[0] else {
        panic!("{msgs:?}")
    };
    assert_eq!(board.project.key, "WB");
    assert_eq!(board.issues.len(), 2);
    assert_eq!(
        paths(&seen),
        ["GET /api/v1/projects/p1", "GET /api/v1/projects/p1/issues"]
    );
    let msgs = block_on(c.execute(Request::Board("zz".into())));
    assert_eq!(msgs, [Msg::NotFound]);
}

#[quench_test::test]
fn an_issue_page_is_four_calls() {
    let (c, seen, _) = client(routes);
    let msgs = block_on(c.execute(Request::Issue("i1".into())));
    let Msg::Issue(view) = &msgs[0] else {
        panic!("{msgs:?}")
    };
    assert_eq!(view.project.id, "p1");
    assert_eq!(view.issue.seq, 1);
    assert_eq!(seen.borrow().len(), 4);
}

#[quench_test::test]
fn saving_puts_then_transitions_then_reads_again() {
    let (c, seen, bodies) = client(routes);
    let body = UpdateIssue {
        title: "New".into(),
        description: None,
        kind: "task".into(),
        priority: "low".into(),
        assignee: Some("ola".into()),
        estimate: Some(2),
    };
    let msgs = block_on(c.execute(Request::Save {
        issue: "i1".into(),
        body,
        status: Some("done".into()),
    }));
    assert!(matches!(msgs[0], Msg::Issue(_)));
    let calls = paths(&seen);
    assert_eq!(calls[0], "PUT /api/v1/issues/i1");
    assert_eq!(calls[1], "POST /api/v1/issues/i1/transition");
    assert_eq!(calls[2], "GET /api/v1/issues/i1");
    let put = bodies.borrow()[0].clone().unwrap();
    assert!(
        put.contains(r#""title":"New""#) && put.contains(r#""assignee":"ola""#),
        "{put}"
    );
    assert_eq!(bodies.borrow()[1].as_deref(), Some(r#"{"status":"done"}"#));
}

#[quench_test::test]
fn creating_returns_what_the_service_made() {
    let (c, _, bodies) = client(routes);
    let msgs = block_on(c.execute(Request::CreateProject(CreateProject {
        key: "WB".into(),
        name: "Workbench".into(),
        description: None,
    })));
    assert!(matches!(&msgs[0], Msg::ProjectCreated(p) if p.id == "p1"));
    assert!(
        bodies.borrow()[0]
            .as_ref()
            .unwrap()
            .contains(r#""key":"WB""#)
    );
    let msgs = block_on(c.execute(Request::CreateIssue {
        project: "p1".into(),
        body: CreateIssue {
            kind: "task".into(),
            title: "T".into(),
            description: None,
            priority: "medium".into(),
            assignee: None,
            estimate: None,
        },
    }));
    assert!(matches!(&msgs[0], Msg::IssueCreated(i) if i.seq == 3));
}

#[quench_test::test]
fn a_link_is_found_by_key_then_made() {
    let (c, seen, bodies) = client(routes);
    let msgs = block_on(c.execute(Request::AddLink {
        issue: "i1".into(),
        key: "WB-2".into(),
        kind: "blocks".into(),
    }));
    assert!(matches!(msgs[0], Msg::Issue(_)));
    let calls = paths(&seen);
    assert_eq!(calls[0], "GET /api/v1/issues?key=WB-2");
    assert_eq!(calls[1], "POST /api/v1/issues/i1/links");
    assert_eq!(
        bodies.borrow()[1].as_deref(),
        Some(r#"{"linked_issue_id":"i2","kind":"blocks"}"#)
    );

    let msgs = block_on(c.execute(Request::AddLink {
        issue: "i1".into(),
        key: "WB-9".into(),
        kind: "blocks".into(),
    }));
    assert_eq!(msgs, [Msg::Rejected("there is no issue WB-9".into())]);

    let msgs = block_on(c.execute(Request::RemoveLink {
        issue: "i1".into(),
        link: "l1".into(),
    }));
    assert!(matches!(msgs[0], Msg::Issue(_)));
}

#[quench_test::test]
fn a_comment_is_posted_then_the_page_read_again() {
    let (c, seen, _) = client(routes);
    let msgs = block_on(c.execute(Request::Comment {
        issue: "i1".into(),
        body: "hi".into(),
    }));
    assert!(matches!(msgs[0], Msg::Issue(_)));
    assert_eq!(paths(&seen)[0], "POST /api/v1/issues/i1/comments");
}

#[quench_test::test]
fn a_move_says_nothing_when_it_worked_and_why_when_it_did_not() {
    let (c, _, _) = client(routes);
    let ok = block_on(c.execute(Request::Move {
        issue: "i1".into(),
        status: "done".into(),
    }));
    assert!(ok.is_empty());

    let (c, _, _) = client(|_| (403, r#"{"error":"no write access to this issue"}"#.into()));
    let msgs = block_on(c.execute(Request::Move {
        issue: "i1".into(),
        status: "done".into(),
    }));
    assert_eq!(
        msgs,
        [Msg::Rejected("no write access to this issue".into())]
    );
}

#[quench_test::test]
fn the_statuses_mean_what_a_screen_can_say() {
    let (c, _, _) = client(|_| (401, String::new()));
    assert_eq!(block_on(c.execute(Request::Projects)), [Msg::SignedOut]);
    assert_eq!(block_on(c.execute(Request::Startup)), [Msg::SignedOut]);
    let (c, _, _) = client(|_| (403, String::new()));
    assert!(
        matches!(&block_on(c.execute(Request::Projects))[0], Msg::Failed(m) if m.contains("gatehouse"))
    );
    let (c, _, _) = client(|_| (502, "<html>".into()));
    assert_eq!(
        block_on(c.execute(Request::Projects)),
        [Msg::Failed("the service answered 502".into())]
    );
    let (c, _, _) = client(|_| (200, "<html>login</html>".into()));
    assert!(
        matches!(&block_on(c.execute(Request::Projects))[0], Msg::Failed(m) if m.contains("unreadable"))
    );
    let (c, _, _) = client(|_| {
        (
            409,
            r#"{"error":"a record with that identity already exists"}"#.into(),
        )
    });
    let msgs = block_on(c.execute(Request::CreateProject(CreateProject {
        key: "WB".into(),
        name: "x".into(),
        description: None,
    })));
    assert_eq!(
        msgs,
        [Msg::Rejected(
            "a record with that identity already exists".into()
        )]
    );
}

#[quench_test::test]
fn the_stylesheet_is_the_apps() {
    assert!(workbench_web::stylesheet().contains(".wb-board"));
}

quench_test::main!();
