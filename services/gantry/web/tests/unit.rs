#![allow(clippy::unwrap_used, clippy::too_many_lines, clippy::type_complexity)]

use std::cell::RefCell;
use std::rc::Rc;

use gantry_app::{Msg, Request};
use gantry_core::ResourceRef;
use gantry_web::client::{Boxed, Client, HttpRequest, HttpResponse, Transport, base_of, segment};
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

fn group_json(package: &str, sync: &str) -> String {
    format!(
        r#"{{"package":"{package}","description":null,"installed":"1.0.0","offered":"1.1.0","status":"update_available","inventoried":true,
        "rows":[{{"apiVersion":"v1","kind":"ConfigMap","name":"c","namespace":"ml","state":"edited","ready":null,"workload":false,"editable":true}}],
        "summary":{{"sync":"{sync}","total":1,"synced":0,"edited":1,"missing":0,"extra":0,"workloads":0,"not_ready":0,"update_to":"1.1.0"}},
        "can":["read","deploy","scale"]}}"#
    )
}

const OPERATION: &str = r#"{"id":"o1","kind":"upgrade","title":"Upgrade ml","state":"running","requested_by":"ana","error":null,"log":null,
    "live_log":"hi","created_at":"2026-10-29T10:00:00Z","plan":{"steps":[],"summary":[]},"step_lines":["download ml 1.1.0"]}"#;

fn routes(request: &HttpRequest) -> (u16, String) {
    let body = match (request.method, request.path.as_str()) {
        ("GET", "/api/v1/info") => {
            r#"{"service":"gantry","version":"0.1","actor":"ana","can":["read","deploy"]}"#.into()
        }
        ("GET", "/api/v1/resources") => {
            format!("[{},{}]", group_json("ml", "out_of_sync"), group_json("media", "synced"))
        }
        ("GET", "/api/v1/operations?limit=10") => {
            format!(r#"[{OPERATION},{}]"#, OPERATION.replace("o1", "o0").replace("running", "succeeded"))
        }
        ("GET", "/api/v1/operations?limit=100") => format!("[{OPERATION}]"),
        ("GET", "/api/v1/operations/o1") => OPERATION.to_string(),
        ("POST", "/api/v1/operations/o1/cancel") => r#"{"id":"o1","title":"Upgrade ml","state":"cancelled"}"#.into(),
        ("GET", "/api/v1/deployments") => {
            r#"[{"name":"inference","declared":true,"target":"ml","description":null,"resources":[],"default":"running","desired":"running",
                "observed":"running","drift":false,"conflicts_with":["training"],"conflicting_running":[]}]"#
                .into()
        }
        ("GET", "/api/v1/targets/ml") => {
            r#"{"name":"ml","description":null,"namespace":"ml","installed":"1.0.0","offered":"1.1.0","status":"update_available","units":[],
                "versions":[{"name":"ml","version":"1.1.0","description":null,"yanked":false}]}"#
                .into()
        }
        ("GET", "/api/v1/plans/p1") => {
            r#"{"id":"p1","target":"ml","action":"upgrade","version":"1.1.0","basis":"1.0.0","plan":{"steps":[],"summary":["x"]},
                "created_by":"ana","created_at":"2026-10-29T10:00:00Z","operation_id":null,"step_lines":["a"],"can_confirm":true}"#
                .into()
        }
        ("GET", p) if p.starts_with("/api/v1/resources/yaml?") => {
            r#"{"yaml":"kind: ConfigMap\n","editable":true}"#.into()
        }
        ("POST", "/api/v1/deployments/training/plan" | "/api/v1/targets/ml/plan") => {
            r#"{"id":"p9","target":"ml","action":"swap"}"#.into()
        }
        ("POST", "/api/v1/plans/p9/confirm" | "/api/v1/plans/p1/confirm")
        | ("POST", "/api/v1/resources/apply" | "/api/v1/resources/delete"
            | "/api/v1/resources/edit" | "/api/v1/resources/refresh") => {
            return (201, r#"{"id":"o5","title":"x","state":"queued"}"#.into());
        }
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
    assert_eq!(base_of("/gantry/ui/home"), "/gantry");
    assert_eq!(base_of("/gantry/ui/operations/x"), "/gantry");
    assert_eq!(base_of("/gantry/ui"), "/gantry");
    assert_eq!(base_of("/ui/home"), "");
    assert_eq!(segment("a b/c"), "a%20b%2Fc");
}

#[quench_test::test]
fn startup_asks_who_is_there() {
    let (c, seen, _) = client(routes);
    let msgs = block_on(c.execute(Request::Startup));
    assert!(matches!(&msgs[..], [Msg::Info(i)] if i.actor == "ana" && i.may("deploy")));
    assert_eq!(paths(&seen), ["GET /api/v1/info"]);
    // Not knowing is not an error; being signed out is.
    let (c, _, _) = client(|_| (500, String::new()));
    assert!(block_on(c.execute(Request::Startup)).is_empty());
    let (c, _, _) = client(|_| (401, String::new()));
    assert_eq!(block_on(c.execute(Request::Startup)), [Msg::SignedOut]);
}

#[quench_test::test]
fn the_packages_are_the_resources_and_the_change_in_progress() {
    let (c, seen, _) = client(routes);
    let msgs = block_on(c.execute(Request::Packages));
    assert!(
        matches!(&msgs[0], Msg::Groups(g) if g.len() == 2 && g[0].summary.sync == "out_of_sync")
    );
    let Msg::Busy(Some(busy)) = &msgs[1] else {
        panic!("{msgs:?}")
    };
    assert_eq!(
        (busy.id.as_str(), busy.title.as_str()),
        ("o1", "Upgrade ml")
    );
    assert_eq!(
        paths(&seen),
        ["GET /api/v1/resources", "GET /api/v1/operations?limit=10"]
    );
}

#[quench_test::test]
fn not_being_allowed_to_see_the_history_only_leaves_out_the_hint() {
    let (c, _, _) = client(|r| {
        if r.path.starts_with("/api/v1/operations") {
            (403, r#"{"error":"no read access here"}"#.into())
        } else {
            routes(r)
        }
    });
    let msgs = block_on(c.execute(Request::Packages));
    assert!(matches!(&msgs[..], [Msg::Groups(_), Msg::Busy(None)]));
}

#[quench_test::test]
fn a_cluster_that_cannot_be_reached_is_said_in_the_services_words() {
    let (c, _, _) = client(|_| (502, r#"{"error":"cluster: connection refused"}"#.into()));
    assert_eq!(
        block_on(c.execute(Request::Packages)),
        [Msg::Failed("cluster: connection refused".into())]
    );
    let (c, _, _) = client(|_| (401, String::new()));
    assert_eq!(block_on(c.execute(Request::Packages)), [Msg::SignedOut]);
    let (c, _, _) = client(|_| (200, "<html>login</html>".into()));
    assert!(
        matches!(&block_on(c.execute(Request::Packages))[0], Msg::Failed(m) if m.contains("unreadable"))
    );
}

#[quench_test::test]
fn a_package_is_asked_for_with_its_deployments_and_an_unknown_one_is_not_found() {
    let (c, seen, _) = client(routes);
    let msgs = block_on(c.execute(Request::Package("ml".into())));
    assert!(matches!(&msgs[0], Msg::Groups(_)));
    assert!(matches!(&msgs[1], Msg::Deployments(d) if d.len() == 1 && d[0].can_stop()));
    assert_eq!(
        paths(&seen),
        ["GET /api/v1/resources", "GET /api/v1/deployments"]
    );

    let msgs = block_on(c.execute(Request::Package("nope".into())));
    assert_eq!(msgs.last(), Some(&Msg::NotFound));

    // Without the deployments the page still shows its resources.
    let (c, _, _) = client(|r| {
        if r.path == "/api/v1/deployments" {
            (500, String::new())
        } else {
            routes(r)
        }
    });
    let msgs = block_on(c.execute(Request::Package("ml".into())));
    assert!(matches!(&msgs[1], Msg::Deployments(d) if d.is_empty()));
}

#[quench_test::test]
fn targets_operations_and_plans_are_read_from_their_endpoints() {
    let (c, seen, _) = client(routes);
    let msgs = block_on(c.execute(Request::Target("ml".into())));
    assert!(matches!(&msgs[0], Msg::Target(t) if t.versions.len() == 1));
    let msgs = block_on(c.execute(Request::Operations));
    assert!(matches!(&msgs[0], Msg::Operations(o) if o.len() == 1));
    let msgs = block_on(c.execute(Request::Operation("o1".into())));
    assert!(
        matches!(&msgs[0], Msg::Operation(o) if o.live_log.as_deref() == Some("hi") && !o.is_terminal())
    );
    let msgs = block_on(c.execute(Request::Plan("p1".into())));
    assert!(matches!(&msgs[0], Msg::Plan(p) if p.can_confirm && p.step_lines == ["a"]));
    assert_eq!(
        paths(&seen),
        [
            "GET /api/v1/targets/ml",
            "GET /api/v1/operations?limit=100",
            "GET /api/v1/operations/o1",
            "GET /api/v1/plans/p1"
        ]
    );
    for request in [
        Request::Target("x".into()),
        Request::Operation("x".into()),
        Request::Plan("x".into()),
    ] {
        assert_eq!(block_on(c.execute(request)), [Msg::NotFound]);
    }
}

#[quench_test::test]
fn the_yaml_is_asked_for_under_the_apis_names() {
    let (c, seen, _) = client(routes);
    let resource = ResourceRef {
        package: "ml".into(),
        api_version: "apps/v1".into(),
        kind: "Deployment".into(),
        name: "sage".into(),
        namespace: String::new(),
    };
    let msgs = block_on(c.execute(Request::Yaml(resource)));
    assert!(matches!(&msgs[0], Msg::Yaml(y) if y.editable && y.yaml.contains("ConfigMap")));
    assert_eq!(
        paths(&seen),
        ["GET /api/v1/resources/yaml?package=ml&apiVersion=apps%2Fv1&kind=Deployment&name=sage"]
    );
}

#[quench_test::test]
fn starting_stopping_and_syncing_plan_and_run_at_once() {
    let (c, seen, bodies) = client(routes);
    let msgs = block_on(c.execute(Request::Start("training".into())));
    assert_eq!(msgs, [Msg::Started("o5".into())]);
    assert_eq!(
        paths(&seen),
        [
            "POST /api/v1/deployments/training/plan",
            "POST /api/v1/plans/p9/confirm"
        ]
    );
    assert_eq!(
        bodies.borrow()[0].as_deref(),
        Some(r#"{"action":"start","stop":[]}"#)
    );

    let (c, seen, bodies) = client(routes);
    block_on(c.execute(Request::Sync {
        package: "ml".into(),
        version: Some("1.1.0".into()),
    }));
    assert_eq!(
        paths(&seen),
        [
            "POST /api/v1/targets/ml/plan",
            "POST /api/v1/plans/p9/confirm"
        ]
    );
    assert_eq!(
        bodies.borrow()[0].as_deref(),
        Some(r#"{"version":"1.1.0"}"#)
    );
    let (c, _, bodies) = client(routes);
    block_on(c.execute(Request::Sync {
        package: "ml".into(),
        version: None,
    }));
    assert_eq!(bodies.borrow()[0].as_deref(), Some(r#"{"version":null}"#));

    // The stop of a deployment says stop.
    let (c, _, bodies) = client(routes);
    block_on(c.execute(Request::Stop("training".into())));
    assert!(
        bodies.borrow()[0]
            .as_deref()
            .unwrap()
            .contains(r#""action":"stop""#)
    );
}

#[quench_test::test]
fn a_plan_that_cannot_be_confirmed_or_made_is_a_reason_not_a_failure() {
    // The caller may plan (read) but not run: the service's 403 is the message.
    let (c, _, _) = client(|r| {
        if r.path.ends_with("/confirm") {
            (403, r#"{"error":"no scale access to ml"}"#.into())
        } else {
            routes(r)
        }
    });
    assert_eq!(
        block_on(c.execute(Request::Start("training".into()))),
        [Msg::Rejected("no scale access to ml".into())]
    );
    let (c, seen, _) = client(|r| {
        if r.path.ends_with("/plan") {
            (422, r#"{"error":"inference is already running"}"#.into())
        } else {
            routes(r)
        }
    });
    assert_eq!(
        block_on(c.execute(Request::Start("inference".into()))),
        [Msg::Rejected("inference is already running".into())]
    );
    assert_eq!(seen.borrow().len(), 1, "nothing is confirmed");
}

#[quench_test::test]
fn a_stored_plan_is_confirmed() {
    let (c, seen, _) = client(routes);
    assert_eq!(
        block_on(c.execute(Request::Confirm("p1".into()))),
        [Msg::Started("o5".into())]
    );
    assert_eq!(paths(&seen), ["POST /api/v1/plans/p1/confirm"]);
    let (c, _, _) = client(|_| {
        (
            409,
            r#"{"error":"this plan has already been confirmed"}"#.into(),
        )
    });
    assert_eq!(
        block_on(c.execute(Request::Confirm("p1".into()))),
        [Msg::Rejected("this plan has already been confirmed".into())]
    );
}

#[quench_test::test]
fn the_resource_buttons_post_what_the_api_reads() {
    let resource = ResourceRef {
        package: "ml".into(),
        api_version: "apps/v1".into(),
        kind: "Deployment".into(),
        name: "sage".into(),
        namespace: "ml".into(),
    };
    let (c, seen, bodies) = client(routes);
    for request in [
        Request::DeleteResource(resource.clone()),
        Request::ApplyResource(resource.clone()),
        Request::ApplyMissing("ml".into()),
        Request::Refresh("ml".into()),
        Request::Edit {
            resource,
            yaml: "kind: Deployment\n".into(),
        },
    ] {
        assert_eq!(block_on(c.execute(request)), [Msg::Started("o5".into())]);
    }
    assert_eq!(
        paths(&seen),
        [
            "POST /api/v1/resources/delete",
            "POST /api/v1/resources/apply",
            "POST /api/v1/resources/apply",
            "POST /api/v1/resources/refresh",
            "POST /api/v1/resources/edit",
        ]
    );
    let bodies: Vec<String> = bodies.borrow().iter().map(|b| b.clone().unwrap()).collect();
    assert_eq!(
        bodies[0],
        r#"{"package":"ml","apiVersion":"apps/v1","kind":"Deployment","name":"sage","namespace":"ml"}"#
    );
    assert_eq!(bodies[2], r#"{"package":"ml"}"#);
    assert!(
        bodies[4].ends_with(r#""yaml":"kind: Deployment\n"}"#),
        "{}",
        bodies[4]
    );
}

#[quench_test::test]
fn a_refused_edit_says_why() {
    let (c, _, _) = client(|_| {
        (
            422,
            r#"{"error":"the YAML describes other/x, not Deployment/sage"}"#.into(),
        )
    });
    let msgs = block_on(c.execute(Request::Edit {
        resource: ResourceRef {
            package: "ml".into(),
            kind: "Deployment".into(),
            name: "sage".into(),
            ..ResourceRef::default()
        },
        yaml: "x".into(),
    }));
    assert_eq!(
        msgs,
        [Msg::Rejected(
            "the YAML describes other/x, not Deployment/sage".into()
        )]
    );
    let (c, _, _) = client(|_| (401, String::new()));
    assert_eq!(
        block_on(c.execute(Request::ApplyMissing("ml".into()))),
        [Msg::SignedOut]
    );
}

#[quench_test::test]
fn cancelling_posts_then_reads_the_operation_again() {
    let (c, seen, _) = client(routes);
    let msgs = block_on(c.execute(Request::Cancel("o1".into())));
    assert!(matches!(&msgs[0], Msg::Operation(o) if o.step_lines.len() == 1));
    assert_eq!(msgs[1], Msg::Done("cancelling".into()));
    assert_eq!(
        paths(&seen),
        [
            "POST /api/v1/operations/o1/cancel",
            "GET /api/v1/operations/o1"
        ]
    );
    let (c, _, _) = client(|_| (403, r#"{"error":"no deploy access here"}"#.into()));
    assert_eq!(
        block_on(c.execute(Request::Cancel("o1".into()))),
        [Msg::Rejected("no deploy access here".into())]
    );
}

#[quench_test::test]
fn language_is_the_browsers_own_business() {
    let (c, seen, _) = client(routes);
    assert!(block_on(c.execute(Request::Language("pl-PL".into()))).is_empty());
    assert!(seen.borrow().is_empty());
}

#[quench_test::test]
fn the_stylesheet_is_the_apps() {
    assert!(gantry_web::stylesheet().contains(".gt-app"));
}

quench_test::main!();
