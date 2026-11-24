//! Proves what the `auth` check does with a route that rejected its probe, and with the mutating routes of a deployment it
//! must not change: parameters from the service's own document, an empty request only where it cannot do anything, and
//! no request at all where it might.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use forge_sectest::census::Route;
use forge_sectest::finding::{Finding, Severity};
use forge_sectest::scan::{self, Options};
use forge_sectest::wire::Target;
use quench_core::async_trait;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::server::serve_http_on;
use quench_proto::StatusCode;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

const DOCUMENT: &str = r"
openapi: 3.1.0
info: {title: Pages, version: 1.0.0}
paths:
  /ui/verify:
    get:
      operationId: verify
      parameters:
      - {name: token, in: query, required: true, schema: {type: string}}
      responses:
        '200': {description: verified}
  /api/v1/things:
    get:
      operationId: listThings
      responses:
        '200': {description: the things}
    post:
      operationId: createThing
      requestBody:
        required: true
        content:
          application/json:
            schema: {type: object}
      responses:
        '201': {description: made}
  /api/v1/things/{id}:
    delete:
      operationId: deleteThing
      parameters:
      - {name: id, in: path, required: true, schema: {type: string}}
      responses:
        '204': {description: gone}
  /api/v1/rotate:
    post:
      operationId: rotate
      responses:
        '204': {description: rotated}
";

/// Everything mutating is guarded; `/ui/verify` is a public link target that wants its token first.
struct Pages {
    seen: Mutex<Vec<String>>,
}

fn reply(status: StatusCode) -> Response {
    Response::text(status, "x").header("x-content-type-options", "nosniff")
}

#[async_trait]
impl Endpoint for Pages {
    async fn call(&self, req: Request) -> Response {
        let method = req.method().as_str().to_string();
        let path = req.uri().path().to_string();
        let query = req.uri().query().unwrap_or("").to_string();
        let signed = req.header("authorization").is_some();
        self.seen
            .lock()
            .unwrap()
            .push(format!("{method} {path} auth={signed}"));
        match (method.as_str(), path.as_str()) {
            ("GET", "/ui/verify") if query.contains("token=") => reply(StatusCode::OK),
            ("GET", "/ui/verify") => reply(StatusCode::BAD_REQUEST),
            ("GET" | "POST", "/api/v1/things") | ("POST", "/api/v1/rotate") => {
                reply(StatusCode::UNAUTHORIZED)
            }
            ("DELETE", p) if p.starts_with("/api/v1/things/") => reply(StatusCode::UNAUTHORIZED),
            _ => Response::not_found(),
        }
    }
}

async fn serve() -> (u16, Arc<Pages>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = Arc::new(Pages {
        seen: Mutex::new(Vec::new()),
    });
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    tokio::spawn(serve_http_on(
        app.clone(),
        container,
        listener,
        std::future::pending::<()>(),
    ));
    (port, app)
}

fn options(port: u16) -> Options {
    let root = std::env::temp_dir().join(format!("forge-sectest-probe-{}", std::process::id()));
    let dir = root.join("docs/services/pages");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("openapi.yaml"), DOCUMENT).unwrap();
    let target = Target::parse("pages", &format!("http://127.0.0.1:{port}")).unwrap();
    let routes = [
        ("GET", "/ui/verify"),
        ("GET", "/api/v1/things"),
        ("POST", "/api/v1/things"),
        ("DELETE", "/api/v1/things/{id}"),
        ("POST", "/api/v1/rotate"),
    ]
    .iter()
    .map(|(method, pattern)| Route::new("pages", method, pattern))
    .collect();
    let mut options = Options::new(vec![target], routes);
    options.rps = 5000;
    options.forge_root = Some(root);
    options.checks = vec!["auth".to_string(), "tokens".to_string()];
    options
}

fn report(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[quench_test::test]
async fn a_route_that_rejected_the_probe_is_asked_again_with_what_its_document_requires() {
    let (port, _) = serve().await;
    let (findings, _) = scan::run(options(port)).await.unwrap();
    let shown = report(&findings);
    assert!(
        findings
            .iter()
            .any(|f| f.check == "auth" && f.path == "/ui/verify" && f.severity >= Severity::Medium),
        "the link target answers once it has its token, so it is decided (here: not in the baseline):\n{shown}"
    );
    assert!(
        findings
            .iter()
            .all(|f| !(f.detail.contains("rejected the probe") && f.detail.contains("/ui/verify"))),
        "it must no longer be listed as undecided:\n{shown}"
    );
}

#[quench_test::test]
async fn mutating_routes_are_probed_only_where_an_empty_request_cannot_do_anything() {
    let (port, app) = serve().await;
    let mut opts = options(port);
    opts.probe_mutating = vec!["pages".to_string()];
    let (findings, _) = scan::run(opts).await.unwrap();
    let seen = app.seen.lock().unwrap().clone();
    let shown = report(&findings);
    assert!(
        seen.iter().any(|s| s.starts_with("POST /api/v1/things ")),
        "a route with a required body is asked, with an empty one: {seen:?}"
    );
    assert!(
        seen.iter().any(|s| s.starts_with("DELETE /api/v1/things/")),
        "a route with a path id is asked, with the sentinel: {seen:?}"
    );
    assert!(
        seen.iter().all(|s| !s.contains("/api/v1/rotate")),
        "a route with no id and no required body could just run, and is never asked: {seen:?}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.detail.contains("not probed") && f.detail.contains("POST /api/v1/rotate")),
        "the route left alone must be named:\n{shown}"
    );
    assert!(
        seen.iter()
            .filter(|s| !s.starts_with("GET "))
            .all(|s| s.ends_with("auth=false")),
        "what was probed was never sent a credential, forged or not (the tokens check stays off mutating routes): {seen:?}"
    );
}

#[quench_test::test]
async fn without_the_switch_no_mutating_route_is_asked_at_all() {
    let (port, app) = serve().await;
    let (findings, _) = scan::run(options(port)).await.unwrap();
    let seen = app.seen.lock().unwrap().clone();
    assert!(
        seen.iter().all(|s| s.starts_with("GET ")),
        "a read-only scan sent a mutating request: {seen:?}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.detail.contains("--probe-mutating")),
        "the note should say how to include them:\n{}",
        report(&findings)
    );
}

quench_test::main!();
