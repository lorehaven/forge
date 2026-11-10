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

fn secret_json(id: &str, repo: Option<&str>, name: &str) -> String {
    let repo = repo.map_or_else(|| "null".to_string(), |r| format!("\"{r}\""));
    format!(
        r#"{{"id":"{id}","repo_id":{repo},"name":"{name}","created_by":"admin","created_at":"{T}","updated_at":"{T}"}}"#
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
        ("GET", "/api/v1/secrets") => format!("[{}]", secret_json("s1", None, "ANVIL_APP_TOKEN")),
        ("GET", "/api/v1/repos/r1/secrets") => {
            format!("[{}]", secret_json("s2", Some("r1"), "RELEASE_GIT_TOKEN"))
        }
        ("GET", "/api/v1/repos/r2/secrets") => {
            return (403, r#"{"error":"no read access to this repository's secrets"}"#.into());
        }
        ("PUT", "/api/v1/secrets/NEW_ONE") => secret_json("s3", None, "NEW_ONE"),
        ("PUT", "/api/v1/repos/r1/secrets/RELEASE_GIT_TOKEN") => {
            secret_json("s2", Some("r1"), "RELEASE_GIT_TOKEN")
        }
        ("DELETE", "/api/v1/secrets/NEW_ONE" | "/api/v1/repos/r1/secrets/RELEASE_GIT_TOKEN") => {
            String::new()
        }
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
        assert_eq!(
            block_on(client.execute(request)),
            [] as [conveyor_app::Msg; 0]
        );
    }
    assert!(seen.borrow().is_empty());
}

quench_test::main!();

fn with_global(request: &HttpRequest) -> (u16, String) {
    if request.method == "GET" && request.path == "/api/v1/access" {
        return (
            200,
            r#"{"username":"ola","read":["p1"],"write":["p1"],"global_read":true,"global_write":true}"#
                .to_string(),
        );
    }
    routes(request)
}

#[quench_test::test]
fn the_secrets_page_reads_the_estate_wide_names_and_each_readable_repository() {
    let (client, seen, _) = client(with_global);
    let msgs = block_on(client.execute(Request::Secrets));
    assert!(matches!(msgs[0], Msg::Access(_)));
    assert!(matches!(msgs[1], Msg::Projects(_)));
    assert!(matches!(msgs[2], Msg::Repos(_)));
    let Msg::Secrets(list) = &msgs[3] else {
        panic!("{msgs:?}")
    };
    // r2 refused its list: it is left out, the page is not an error.
    let names: Vec<_> = list.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["ANVIL_APP_TOKEN", "RELEASE_GIT_TOKEN"]);
    assert_eq!(list[0].repo_id, None);
    assert_eq!(list[1].repo_id.as_deref(), Some("r1"));
    assert!(
        seen.borrow()
            .iter()
            .any(|(_, path)| path == "/api/v1/secrets")
    );
}

#[quench_test::test]
fn without_the_blanket_grant_the_estate_wide_secrets_are_not_asked_for() {
    // `routes` answers `/access` without the flags: both are false.
    let (client, seen, _) = client(routes);
    let msgs = block_on(client.execute(Request::Secrets));
    let Msg::Secrets(list) = msgs.last().unwrap() else {
        panic!("{msgs:?}")
    };
    assert_eq!(list.len(), 1);
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|(_, path)| path == "/api/v1/secrets")
    );
}

#[quench_test::test]
fn setting_and_removing_a_secret_send_the_value_once_and_read_the_names_again() {
    let (client, seen, bodies) = client(with_global);
    let msgs = block_on(client.execute(Request::SetSecret {
        repo: None,
        name: "NEW_ONE".into(),
        value: "s3cret-value".into(),
    }));
    assert_eq!(msgs.last(), Some(&Msg::Done("ui_secrets_ok_set".into())));
    assert!(msgs.iter().any(|m| matches!(m, Msg::Secrets(_))));
    {
        let seen = seen.borrow();
        assert_eq!(
            seen[0],
            ("PUT".to_string(), "/api/v1/secrets/NEW_ONE".to_string())
        );
        assert!(
            seen.iter()
                .any(|(m, p)| m == "GET" && p == "/api/v1/secrets")
        );
    }
    assert_eq!(
        bodies.borrow()[0].as_deref(),
        Some(r#"{"value":"s3cret-value"}"#)
    );

    let msgs = block_on(client.execute(Request::SetSecret {
        repo: Some("r1".into()),
        name: "RELEASE_GIT_TOKEN".into(),
        value: "another".into(),
    }));
    assert_eq!(msgs.last(), Some(&Msg::Done("ui_secrets_ok_set".into())));
    assert!(seen.borrow().contains(&(
        "PUT".to_string(),
        "/api/v1/repos/r1/secrets/RELEASE_GIT_TOKEN".to_string()
    )));

    let msgs = block_on(client.execute(Request::DeleteSecret {
        repo: None,
        name: "NEW_ONE".into(),
    }));
    assert_eq!(
        msgs.last(),
        Some(&Msg::Done("ui_secrets_ok_removed".into()))
    );
}

#[quench_test::test]
fn a_refused_secret_change_says_why_and_does_not_read_again() {
    let (client, seen, _) = client(|r| {
        if r.method == "PUT" {
            (
                403,
                r#"{"error":"no write access to estate-wide secrets"}"#.into(),
            )
        } else {
            with_global(r)
        }
    });
    let msgs = block_on(client.execute(Request::SetSecret {
        repo: None,
        name: "NEW_ONE".into(),
        value: "abcd".into(),
    }));
    assert_eq!(
        msgs,
        vec![Msg::Rejected(
            "no write access to estate-wide secrets".into()
        )]
    );
    assert_eq!(seen.borrow().len(), 1);
}
