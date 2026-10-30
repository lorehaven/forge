#![allow(clippy::unwrap_used, clippy::too_many_lines, clippy::type_complexity)]

use std::cell::RefCell;
use std::rc::Rc;

use conveyor_app::{Follow, Msg, Request};
use conveyor_core::{LaunchDraft, RepoDraft};
use conveyor_web::client::{Boxed, Client, HttpRequest, HttpResponse, Transport, base_of};
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

const T: &str = "2026-10-29T10:00:00Z";

fn project_json(id: &str) -> String {
    format!(
        r#"{{"id":"{id}","name":"{id}","parent_id":null,"created_at":"{T}","updated_at":"{T}"}}"#
    )
}

fn repo_json(id: &str, name: &str) -> String {
    format!(
        r#"{{"id":"{id}","provider":"github","owner":"lorehaven","name":"{name}","clone_url":"https://x/{name}.git","default_branch":"master","registered_by":"a","project_id":"p1","enabled":true,"created_at":"{T}","updated_at":"{T}"}}"#
    )
}

fn run_json(id: &str, status: &str) -> String {
    format!(
        r#"{{"id":"{id}","repo_id":"r1","trigger":"push","git_ref":"refs/heads/master","sha":"abcdef0123","message":null,"delivery_id":null,"status":"{status}","queued_at":"{T}","started_at":null,"finished_at":null,"claimed_by":null,"claimed_at":null,"attempt":1,"error":null,"resumed_from":null,"stages":null}}"#
    )
}

fn routes(request: &HttpRequest) -> (u16, String) {
    let body = match (request.method, request.path.as_str()) {
        ("GET", "/api/v1/access") => r#"{"username":"ola","read":["p1"],"write":["p1"]}"#.to_string(),
        ("GET", "/api/v1/projects?all=true") => format!("[{}]", project_json("p1")),
        ("GET", "/api/v1/repos") => format!("[{},{}]", repo_json("r1", "conveyor"), repo_json("r2", "other")),
        ("GET", "/api/v1/repos/r1") => repo_json("r1", "conveyor"),
        ("GET", "/api/v1/subscriptions") => {
            format!(r#"[{{"id":"s","repo_id":"r1","project_id":null,"created_at":"{T}"}}]"#)
        }
        ("GET", "/api/v1/overview/runs" | "/api/v1/overview/runs?project=p1") => {
            format!("[{}]", run_json("run-1", "success"))
        }
        ("GET", "/api/v1/overview/history?page=2") => {
            format!(r#"{{"runs":[{}],"page":2,"pages":2,"total":30}}"#, run_json("run-1", "success"))
        }
        ("GET", "/api/v1/scan") => r#"[{"repo_id":"r1","checks":[{"kind":"lint","passed":true,"findings":1,"metric":null}]}]"#.to_string(),
        ("GET", "/api/v1/runs/run-1") => {
            let run = run_json("run-1", "failed");
            format!(r#"{},"jobs":[],"artifacts":[]}}"#, &run[..run.len() - 1])
        }
        ("POST", "/api/v1/runs/run-1/restart") => run_json("run-2", "queued"),
        ("POST", "/api/v1/repos/r1/runs") => run_json("run-3", "queued"),
        ("POST", "/api/v1/repos") | ("PATCH", "/api/v1/repos/r1") => repo_json("r1", "renamed"),
        ("DELETE", "/api/v1/repos/r1") => String::new(),
        ("PUT" | "DELETE", "/api/v1/repos/r1/subscription") => r#"{"subscribed":true}"#.to_string(),
        ("GET", "/api/v1/repos/r1/stages") => r#"[{"name":"check","on_by_default":true}]"#.to_string(),
        ("GET", "/api/v1/repos/r1/scan") => format!(r#"{{"run":{},"checks":[]}}"#, run_json("run-1", "success")),
        ("GET", "/api/v1/repos/r1/scan/findings") => return (404, r#"{"error":"no scanner findings"}"#.into()),
        ("GET", "/api/v1/credentials") => "[]".to_string(),
        _ => return (404, r#"{"error":"nothing here"}"#.to_string()),
    };
    (200, body)
}

#[quench_test::test]
fn the_page_path_gives_the_services_prefix() {
    assert_eq!(base_of("/conveyor/ui/home"), "/conveyor");
    assert_eq!(base_of("/ui/home"), "");
    assert_eq!(base_of("/a/b/ui"), "/a/b");
}

#[quench_test::test]
fn the_front_page_reads_projects_repos_runs_and_chips() {
    let (client, seen, _) = client(routes);
    let msgs = block_on(client.execute(Request::Home { scope: None }));
    assert!(matches!(msgs[0], Msg::Projects(_)));
    assert!(matches!(msgs[1], Msg::Repos(ref r) if r.len() == 2));
    assert!(matches!(msgs[2], Msg::Recent { scope: None, ref runs } if runs.len() == 1));
    assert!(matches!(msgs[3], Msg::Chips(ref c) if c[0].checks[0].kind == "lint"));
    assert_eq!(seen.borrow().len(), 4);

    let msgs = block_on(client.execute(Request::Recent {
        scope: Some("p1".into()),
    }));
    assert!(matches!(msgs[0], Msg::Recent { scope: Some(_), .. }));
    // An unknown project is "not found", not a failure.
    let msgs = block_on(client.execute(Request::Recent {
        scope: Some("nope".into()),
    }));
    assert_eq!(msgs, vec![Msg::NotFound]);
}

#[quench_test::test]
fn the_history_asks_for_its_page_and_a_run_brings_its_repository() {
    let (client, seen, _) = client(routes);
    let msgs = block_on(client.execute(Request::History {
        scope: None,
        page: 2,
    }));
    assert!(matches!(msgs.last(), Some(Msg::History(p)) if p.page == 2 && p.pages == 2));
    assert!(
        seen.borrow()
            .iter()
            .any(|(_, p)| p == "/api/v1/overview/history?page=2")
    );

    let msgs = block_on(client.execute(Request::Run("run-1".into())));
    let [Msg::Run(view)] = msgs.as_slice() else {
        panic!("{msgs:?}")
    };
    assert_eq!(view.repo.as_ref().unwrap().name, "conveyor");
    assert_eq!(view.run.status, conveyor_core::Status::Failed);
}

#[quench_test::test]
fn restarting_and_running_by_hand_open_the_new_run() {
    let (client, _, bodies) = client(routes);
    assert_eq!(
        block_on(client.execute(Request::Restart("run-1".into()))),
        vec![Msg::Opened("run-2".into())]
    );
    let draft = LaunchDraft {
        ticked: vec!["check".into()],
        ..LaunchDraft::default()
    };
    let msgs = block_on(client.execute(Request::RunStages {
        repo: "r1".into(),
        body: draft.to_trigger().unwrap(),
    }));
    assert_eq!(msgs, vec![Msg::Opened("run-3".into())]);
    let sent = bodies.borrow().last().cloned().flatten().unwrap();
    assert!(sent.contains(r#""stages":["check"]"#), "{sent}");
}

#[quench_test::test]
fn a_refused_change_says_why_in_the_services_words() {
    let (client, _, _) = client(|r| {
        if r.method == "POST" {
            (
                403,
                r#"{"error":"no write access to this repository"}"#.into(),
            )
        } else {
            routes(r)
        }
    });
    let msgs = block_on(client.execute(Request::RunNow {
        repo: "r1".into(),
        scope: None,
    }));
    assert_eq!(
        msgs,
        vec![Msg::Rejected("no write access to this repository".into())]
    );
    let msgs = block_on(client.execute(Request::Run("nope".into())));
    assert_eq!(msgs, vec![Msg::NotFound]);
}

#[quench_test::test]
fn a_lost_session_is_told_whatever_was_asked() {
    let (client, _, _) = client(|_| (401, String::new()));
    for request in [
        Request::Repos,
        Request::Credentials,
        Request::Home { scope: None },
        Request::DeleteRepo("r1".into()),
    ] {
        assert_eq!(block_on(client.execute(request)), vec![Msg::SignedOut]);
    }
}

#[quench_test::test]
fn the_repository_pages_read_what_they_need_and_changes_read_the_list_again() {
    let (client, seen, bodies) = client(routes);
    let msgs = block_on(client.execute(Request::Repos));
    assert!(matches!(msgs[0], Msg::Access(_)));
    assert!(matches!(msgs.last(), Some(Msg::Subscriptions(s)) if s.len() == 1));

    let draft = RepoDraft {
        owner: "lorehaven".into(),
        name: "x".into(),
        clone_url: "https://x/x.git".into(),
        project_id: "p1".into(),
        ..RepoDraft::default()
    };
    let msgs = block_on(client.execute(Request::CreateRepo(draft.to_register().unwrap())));
    assert!(matches!(msgs[0], Msg::Repos(_)));
    assert_eq!(msgs[1], Msg::Done("ui_repos_ok_created".into()));

    let msgs = block_on(client.execute(Request::SaveRepo {
        id: "r1".into(),
        body: draft.to_update().unwrap(),
    }));
    assert!(matches!(&msgs[1], Msg::Saved { name, .. } if name == "renamed"));
    assert!(
        seen.borrow()
            .iter()
            .any(|(m, p)| m == "PATCH" && p == "/api/v1/repos/r1")
    );
    assert!(
        bodies
            .borrow()
            .iter()
            .flatten()
            .any(|b| b.contains(r#""enabled":true"#))
    );

    assert_eq!(
        block_on(client.execute(Request::DeleteRepo("r1".into()))),
        vec![Msg::Deleted]
    );
    let msgs = block_on(client.execute(Request::Follow {
        target: Follow::Repo("r1".into()),
        on: true,
    }));
    assert_eq!(
        msgs.last(),
        Some(&Msg::Done("ui_follow_ok_followed".into()))
    );
    let msgs = block_on(client.execute(Request::Follow {
        target: Follow::Repo("r1".into()),
        on: false,
    }));
    assert_eq!(
        msgs.last(),
        Some(&Msg::Done("ui_follow_ok_unfollowed".into()))
    );
    assert!(
        seen.borrow()
            .iter()
            .any(|(m, p)| m == "DELETE" && p == "/api/v1/repos/r1/subscription")
    );
}

#[quench_test::test]
fn the_launch_and_scan_pages_find_the_repository_by_owner_and_name() {
    let (client, seen, _) = client(routes);
    let msgs = block_on(client.execute(Request::Launch {
        owner: "lorehaven".into(),
        name: "conveyor".into(),
    }));
    assert!(
        matches!(msgs.last(), Some(Msg::Stages { repo, stages }) if repo == "r1" && stages.len() == 1)
    );

    let msgs = block_on(client.execute(Request::Launch {
        owner: "lorehaven".into(),
        name: "nope".into(),
    }));
    assert!(!msgs.iter().any(|m| matches!(m, Msg::Stages { .. })));

    let msgs = block_on(client.execute(Request::Scan {
        owner: "lorehaven".into(),
        name: "conveyor".into(),
    }));
    assert!(matches!(msgs.last(), Some(Msg::Scan { repo, .. }) if repo == "r1"));
    seen.borrow_mut().clear();

    // Only the security check asks for the scanner's findings, and none is an answer, not an error.
    let msgs = block_on(client.execute(Request::Check {
        owner: "lorehaven".into(),
        name: "conveyor".into(),
        kind: "lint".into(),
    }));
    assert!(!msgs.iter().any(|m| matches!(m, Msg::Findings { .. })));
    let msgs = block_on(client.execute(Request::Check {
        owner: "lorehaven".into(),
        name: "conveyor".into(),
        kind: "security".into(),
    }));
    assert_eq!(
        msgs.last(),
        Some(&Msg::Findings {
            repo: "r1".into(),
            found: None
        })
    );
}

#[quench_test::test]
fn credentials_bring_the_names_that_label_them() {
    let (client, _, _) = client(routes);
    let msgs = block_on(client.execute(Request::Credentials));
    assert!(matches!(msgs[0], Msg::Projects(_)));
    assert!(matches!(msgs[1], Msg::Repos(_)));
    assert!(matches!(msgs[2], Msg::Credentials(_)));
}

#[quench_test::test]
fn the_browsers_own_requests_are_not_the_clients() {
    let (client, seen, _) = client(routes);
    for request in [
        Request::Language("pl-PL".into()),
        Request::OpenLog("j".into()),
        Request::CloseLog("j".into()),
    ] {
        assert!(block_on(client.execute(request)).is_empty());
    }
    assert!(seen.borrow().is_empty());
}

quench_test::main!();
