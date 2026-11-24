//! Proves the authorisation check works: it must flag a service that ignores the token's audience, lets a read-only
//! grant write and lets one user into another's data, and it must stay quiet about one that does none of that.
//! A check that has never been shown the bug it hunts proves nothing when it reports a clean bill of health.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, clippy::significant_drop_tightening)]

use forge_sectest::census::Route;
use forge_sectest::finding::{Finding, Severity};
use forge_sectest::identity::{Spec, Step, json_at};
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

const SPEC: &str = r#"
open = ["notes GET /api/v1/me"]

[[service]]
name = "notes"
reader = "notes:read"
writer = "notes:read notes:write"
reads = "grant"

[[resource]]
name = "a note"
service = "notes"
owner = "alice"
other = "bob"
create = 'POST /api/v1/notes :: {"text":"{canary}"}'
id_at = "id"
read = ["GET /api/v1/notes/{id}"]
write = ['PUT /api/v1/notes/{id} :: {"text":"tampered"}', "DELETE /api/v1/notes/{id}"]
cleanup = "DELETE /api/v1/notes/{id}"
"#;

/// A notes service and, on the same port, the mint endpoint gatehouse offers in test mode. A token is `sub|aud|scope`.
struct Notes {
    /// Checks nothing it should: not the audience, not the grant, not the owner.
    flawed: bool,
    notes: Mutex<Vec<(String, String, String)>>,
}

impl Notes {
    const fn new(flawed: bool) -> Self {
        Self {
            flawed,
            notes: Mutex::new(Vec::new()),
        }
    }
}

fn reply(status: StatusCode, body: &str) -> Response {
    Response::text(status, body).header("x-content-type-options", "nosniff")
}

#[async_trait]
impl Endpoint for Notes {
    async fn call(&self, mut req: Request) -> Response {
        let method = req.method().as_str().to_string();
        let path = req.uri().path().to_string();
        let authorization = req.header("authorization").map(str::to_string);
        let body = req
            .take_body()
            .collect_limited(65536)
            .await
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default();

        if method == "POST" && path == "/api/v1/test/token" {
            let value: quench_serde::Value =
                quench_serde::json::from_str(&body).unwrap_or_default();
            let sub = value
                .get("sub")
                .and_then(quench_serde::Value::as_str)
                .unwrap_or("");
            let aud = value
                .get("aud")
                .and_then(|a| a.get(0))
                .and_then(quench_serde::Value::as_str)
                .unwrap_or("");
            let scope = value
                .get("scope")
                .and_then(quench_serde::Value::as_str)
                .unwrap_or("");
            return reply(
                StatusCode::OK,
                &format!(r#"{{"access_token":"{sub}|{aud}|{scope}"}}"#),
            );
        }

        let Some(token) = authorization
            .as_deref()
            .and_then(|a| a.strip_prefix("Bearer "))
        else {
            return reply(StatusCode::UNAUTHORIZED, "no token");
        };
        let parts: Vec<&str> = token.splitn(3, '|').collect();
        let [sub, aud, scope] = parts[..] else {
            return reply(StatusCode::UNAUTHORIZED, "bad token");
        };
        if !self.flawed && aud != "notes" {
            return reply(StatusCode::FORBIDDEN, "wrong audience");
        }
        let may = |grant: &str| self.flawed || scope.split(' ').any(|s| s == grant);

        let id = path.strip_prefix("/api/v1/notes/");
        match (method.as_str(), id) {
            ("GET", None) if path == "/api/v1/me" => reply(StatusCode::OK, sub),
            ("POST", None) if path == "/api/v1/notes" => {
                if !may("notes:write") {
                    return reply(StatusCode::FORBIDDEN, "no write grant");
                }
                let mut notes = self.notes.lock().unwrap();
                let id = format!("n{}", notes.len() + 1);
                notes.push((id.clone(), sub.to_string(), body));
                reply(StatusCode::CREATED, &format!(r#"{{"id":"{id}"}}"#))
            }
            (verb @ ("GET" | "PUT" | "DELETE"), Some(id)) => {
                let grant = if verb == "GET" {
                    "notes:read"
                } else {
                    "notes:write"
                };
                if !may(grant) {
                    return reply(StatusCode::FORBIDDEN, "no grant");
                }
                let mut notes = self.notes.lock().unwrap();
                let Some(at) = notes
                    .iter()
                    .position(|(n, owner, _)| n == id && (self.flawed || owner == sub))
                else {
                    return reply(StatusCode::NOT_FOUND, "no such note");
                };
                match verb {
                    "GET" => reply(StatusCode::OK, &notes[at].2.clone()),
                    "PUT" => {
                        notes[at].2 = body;
                        reply(StatusCode::OK, "saved")
                    }
                    _ => {
                        notes.remove(at);
                        reply(StatusCode::NO_CONTENT, "")
                    }
                }
            }
            _ => Response::not_found(),
        }
    }
}

async fn serve(app: Notes) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    tokio::spawn(serve_http_on(
        Arc::new(app),
        container,
        listener,
        std::future::pending::<()>(),
    ));
    port
}

fn options(port: u16, spec: &str) -> Options {
    let target = Target::parse("notes", &format!("http://127.0.0.1:{port}")).unwrap();
    let routes = [
        ("GET", "/api/v1/me"),
        ("POST", "/api/v1/notes"),
        ("GET", "/api/v1/notes/{id}"),
        ("PUT", "/api/v1/notes/{id}"),
        ("DELETE", "/api/v1/notes/{id}"),
    ]
    .iter()
    .map(|(method, pattern)| Route::new("notes", method, pattern))
    .collect();
    let mut options = Options::new(vec![target], routes);
    options.rps = 2000;
    options.active = true;
    options.checks = vec!["authz".to_string()];
    options.authz = Some(Spec::parse(spec).unwrap());
    options.mint = Some(format!("http://127.0.0.1:{port}"));
    options
}

fn report(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn has(findings: &[Finding], severity: Severity, needle: &str) -> bool {
    findings
        .iter()
        .any(|f| f.check == "authz" && f.severity >= severity && f.detail.contains(needle))
}

#[quench_test::test]
async fn a_flawed_service_is_flagged_on_every_axis() {
    let port = serve(Notes::new(true)).await;
    let (findings, _) = scan::run(options(port, SPEC)).await.unwrap();
    let shown = report(&findings);
    assert!(
        has(&findings, Severity::High, "another service"),
        "an ignored audience was not flagged:\n{shown}"
    );
    assert!(
        has(&findings, Severity::High, "a read grant can change data"),
        "a reader that can write was not flagged:\n{shown}"
    );
    assert!(
        has(&findings, Severity::High, "IDOR: read"),
        "another user's read was not flagged:\n{shown}"
    );
    assert!(
        has(&findings, Severity::Critical, "IDOR: write"),
        "another user's write was not flagged:\n{shown}"
    );
    assert!(
        has(&findings, Severity::Medium, "no grant"),
        "a token without a grant was not flagged:\n{shown}"
    );
}

#[quench_test::test]
async fn a_service_that_checks_everything_produces_nothing_serious() {
    let port = serve(Notes::new(false)).await;
    let (findings, requests) = scan::run(options(port, SPEC)).await.unwrap();
    let shown = report(&findings);
    assert!(
        findings.iter().all(|f| f.severity <= Severity::Info),
        "a correct service was accused:\n{shown}"
    );
    // Mint (7 identities' worth) + the generic probes + the recipe: far more than a scan that quietly did nothing.
    assert!(
        requests > 15,
        "suspiciously few requests: {requests}\n{shown}"
    );
}

#[quench_test::test]
async fn a_route_every_signed_in_user_may_reach_is_listed_not_reported() {
    let port = serve(Notes::new(false)).await;
    let without = SPEC.replace(r#"open = ["notes GET /api/v1/me"]"#, "open = []");
    let (findings, _) = scan::run(options(port, &without)).await.unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f.severity >= Severity::Medium && f.path == "/api/v1/me"),
        "an unlisted open route should be reported:\n{}",
        report(&findings)
    );
}

#[quench_test::test]
async fn reads_that_need_only_a_login_are_a_note_unless_the_service_says_otherwise() {
    let port = serve(Notes::new(false)).await;
    let lenient = SPEC
        .replace("reads = \"grant\"\n", "")
        .replace(r#"open = ["notes GET /api/v1/me"]"#, "open = []");
    let (findings, _) = scan::run(options(port, &lenient)).await.unwrap();
    let shown = report(&findings);
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Low && f.detail.contains("read routes answer")),
        "the grant-less read should be one low note:\n{shown}"
    );
    assert!(
        findings.iter().all(|f| f.severity <= Severity::Low),
        "a documented login-only read must not fail a scan:\n{shown}"
    );
}

#[quench_test::test]
async fn a_stale_recipe_is_reported_not_silently_passed() {
    let port = serve(Notes::new(false)).await;
    let stale = SPEC.replace("POST /api/v1/notes ::", "POST /api/v1/gone ::");
    let (findings, _) = scan::run(options(port, &stale)).await.unwrap();
    assert!(
        has(&findings, Severity::Medium, "recipe is stale"),
        "a recipe that tested nothing must say so:\n{}",
        report(&findings)
    );
}

#[quench_test::test]
async fn without_active_nothing_is_created_or_changed() {
    let port = serve(Notes::new(true)).await;
    let mut opts = options(port, SPEC);
    opts.active = false;
    let (findings, _) = scan::run(opts).await.unwrap();
    assert!(
        findings
            .iter()
            .all(|f| !matches!(f.method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE")),
        "no mutating request may be sent, or reported on, without --active:\n{}",
        report(&findings)
    );
    assert!(
        findings.iter().all(|f| !f.detail.contains("IDOR")),
        "the ownership recipes create data and need --active:\n{}",
        report(&findings)
    );
    assert!(findings.iter().any(|f| f.detail.contains("--active")));
}

#[quench_test::test]
async fn asking_for_authz_without_a_spec_is_an_error() {
    let port = serve(Notes::new(false)).await;
    let mut opts = options(port, SPEC);
    opts.authz = None;
    let error = scan::run(opts).await.unwrap_err();
    assert!(error.contains("--authz"), "{error}");
}

#[quench_test::test]
fn the_spec_is_checked_when_it_is_read() {
    assert!(Spec::parse("").unwrap_err().contains("[[service]]"));
    let same_user = SPEC.replace(r#"other = "bob""#, r#"other = "alice""#);
    assert!(
        Spec::parse(&same_user)
            .unwrap_err()
            .contains("two different users")
    );
    let no_service = SPEC.replace(
        r#"service = "notes"
owner"#,
        r#"service = "nowhere"
owner"#,
    );
    assert!(Spec::parse(&no_service).unwrap_err().contains("nowhere"));
    let spec = Spec::parse(SPEC).unwrap();
    assert_eq!(spec.resources.len(), 1);
    assert_eq!(spec.resources[0].write.len(), 2);
    assert!(spec.open.contains("notes GET /api/v1/me"));
}

#[quench_test::test]
fn a_recipe_that_grants_per_object_is_read_with_its_setup() {
    let text = format!(
        "{SPEC}\n{}",
        r#"
[[resource]]
name = "a project issue"
service = "notes"
owner = "carol"
other = "dave"
prepare = ["GET /api/v1/me"]
setup = 'POST /api/v1/projects :: {"key":"S{nonce}"}'
setup2 = 'POST /api/v1/projects :: {"key":"T{nonce}"}'
setup_at = "id"
owner_scope = "user notes:project:{setup}:write"
other_scope = "user notes:project:{setup2}:write"
create = 'POST /api/v1/projects/{setup}/issues :: {"title":"{canary}"}'
read = ["GET /api/v1/issues/{id}"]
teardown = ["DELETE /api/v1/projects/{setup}"]
"#
    );
    let spec = Spec::parse(&text).unwrap();
    let resource = &spec.resources[1];
    assert!(resource.setup.is_some() && resource.setup2.is_some());
    assert_eq!(resource.prepare.len(), 1);
    assert_eq!(resource.teardown.len(), 1);
    assert_eq!(
        resource.owner_scope.as_deref(),
        Some("user notes:project:{setup}:write")
    );
}

#[quench_test::test]
fn steps_and_ids_are_read_the_way_the_recipes_write_them() {
    let step = Step::parse(r#"PUT /a/{id} :: {"x":1}"#).unwrap();
    assert_eq!(
        (step.method.as_str(), step.path.as_str()),
        ("PUT", "/a/{id}")
    );
    assert_eq!(step.body.as_deref(), Some(r#"{"x":1}"#));
    assert!(Step::parse("GET").is_err());
    assert!(Step::parse("GET nope").is_err());
    assert_eq!(
        json_at(r#"{"task":{"id":"t1"}}"#, "task.id").as_deref(),
        Some("t1")
    );
    assert_eq!(json_at(r#"{"id":7}"#, "id").as_deref(), Some("7"));
    assert_eq!(json_at(r#"{"id":null}"#, "id"), None);
}

quench_test::main!();
