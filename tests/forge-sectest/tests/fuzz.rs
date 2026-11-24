//! Proves the fuzzer works: from nothing but a service's `OpenAPI` document it must find the bugs planted in a flawed
//! service (a 500 on a hostile string, a database error in a reply, an overflow panic, a client-settable `is_admin`)
//! and leave a strict one alone. A fuzzer that has never been shown a bug it finds proves nothing by finding none.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, clippy::significant_drop_tightening)]

use forge_sectest::finding::{Finding, Severity};
use forge_sectest::openapi;
use forge_sectest::scan::{self, Options};
use forge_sectest::wire::Target;
use quench_core::async_trait;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::server::serve_http_on;
use quench_proto::StatusCode;
use quench_serde::Value;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

const DOCUMENT: &str = r"
openapi: 3.1.0
info: {title: Notes, version: 1.0.0}
paths:
  /api/v1/items:
    get:
      operationId: listItems
      responses:
        '200': {description: the items}
    post:
      operationId: createItem
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/NewItem'
            example: {name: first, count: 1}
      responses:
        '201': {description: made}
  /api/v1/items/{id}:
    get:
      operationId: getItem
      parameters:
      - {name: id, in: path, required: true, schema: {type: string}}
      responses:
        '200': {description: found}
  /api/v1/things/{kind}:
    get:
      operationId: getThing
      parameters:
      - {name: kind, in: path, required: true, schema: {type: string}}
      responses:
        '200': {description: found}
  /api/v1/search:
    get:
      operationId: search
      parameters:
      - {name: q, in: query, required: false, schema: {type: string}}
      - {name: limit, in: query, required: false, schema: {type: integer}}
      responses:
        '200': {description: found}
components:
  schemas:
    NewItem:
      type: object
      required: [name]
      properties:
        name: {type: string}
        count: {type: integer}
";

/// `flawed` services take whatever arrives; strict ones validate everything and answer 4xx.
struct Notes {
    flawed: bool,
    items: Mutex<Vec<String>>,
}

fn reply(status: StatusCode, body: &str) -> Response {
    Response::text(status, body)
        .header("content-type", "application/json")
        .header("x-content-type-options", "nosniff")
}

#[async_trait]
impl Endpoint for Notes {
    async fn call(&self, mut req: Request) -> Response {
        let method = req.method().as_str().to_string();
        let path = req.uri().path().to_string();
        let query = req.uri().query().unwrap_or("").to_string();
        let content_type = req.header("content-type").unwrap_or("").to_string();
        let body = req
            .take_body()
            .collect_limited(1 << 20)
            .await
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default();

        if path == "/health" {
            return reply(StatusCode::OK, "{}");
        }
        if method == "GET" && path == "/api/v1/search" {
            if self.flawed && query.contains("%27") {
                return reply(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "syntax error at or near \"'\"",
                );
            }
            return reply(StatusCode::OK, "[]");
        }
        if method == "GET" && path == "/api/v1/items" {
            let stored = self.items.lock().unwrap().len();
            return reply(
                StatusCode::OK,
                if stored > 0 { r#"[{"id":"1"}]"# } else { "[]" },
            );
        }
        if method == "GET" && path.starts_with("/api/v1/things/") {
            if self.flawed && path.contains("%27") {
                return reply(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "syntax error at or near \"'\"",
                );
            }
            let found = path == "/api/v1/things/widget";
            return reply(
                if found {
                    StatusCode::OK
                } else {
                    StatusCode::NOT_FOUND
                },
                "{}",
            );
        }
        if method == "GET" {
            return match path.strip_prefix("/api/v1/items/") {
                Some(id) if self.flawed && id.contains("%27") => reply(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "syntax error at or near \"'\"",
                ),
                Some(id) if self.items.lock().unwrap().iter().any(|i| i == id) => {
                    reply(StatusCode::OK, "{}")
                }
                _ => reply(StatusCode::NOT_FOUND, "{}"),
            };
        }
        if method == "POST" && path == "/api/v1/items" {
            if !content_type.starts_with("application/json") && !self.flawed {
                return reply(StatusCode::UNSUPPORTED_MEDIA_TYPE, "{}");
            }
            let Ok(value) = quench_serde::json::from_str::<Value>(&body) else {
                return reply(StatusCode::BAD_REQUEST, "{}");
            };
            let Some(map) = value.as_object() else {
                return reply(StatusCode::BAD_REQUEST, "{}");
            };
            let name = map.get("name").and_then(Value::as_str);
            let count = map.get("count");
            if self.flawed {
                if name.is_some_and(|n| n.contains('\'')) {
                    return reply(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "syntax error at or near \"'\"",
                    );
                }
                if count.and_then(Value::as_i64) == Some(i64::MAX) {
                    return reply(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "thread 'main' panicked at src/main.rs:1:1: attempt to add with overflow",
                    );
                }
                let mut stored = self.items.lock().unwrap();
                stored.push("1".to_string());
                // Everything the client sent is stored and echoed back: is_admin included.
                let mut stored_item = map.clone();
                stored_item.insert("id", Value::from("1"));
                return reply(
                    StatusCode::CREATED,
                    &quench_serde::json::to_string(&Value::Object(stored_item)),
                );
            }
            let known = map.keys().all(|k| k == "name" || k == "count");
            let typed = name.is_some()
                && count.is_none_or(|c| c.as_i64().is_some() && c.as_i64().unwrap() >= 0);
            if !known || !typed || name.is_some_and(str::is_empty) || body.len() > 4096 {
                return reply(StatusCode::BAD_REQUEST, "{}");
            }
            let mut stored = self.items.lock().unwrap();
            stored.push("1".to_string());
            return reply(StatusCode::CREATED, r#"{"id":"1"}"#);
        }
        Response::not_found()
    }
}

async fn serve(flawed: bool) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    tokio::spawn(serve_http_on(
        Arc::new(Notes {
            flawed,
            items: Mutex::new(Vec::new()),
        }),
        container,
        listener,
        std::future::pending::<()>(),
    ));
    port
}

fn forge_root() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("forge-sectest-fuzz-{}", std::process::id()));
    let dir = root.join("docs/services/notes");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("openapi.yaml"), DOCUMENT).unwrap();
    root
}

fn options(port: u16) -> Options {
    let target = Target::parse("notes", &format!("http://127.0.0.1:{port}")).unwrap();
    let mut options = Options::new(vec![target], Vec::new());
    options.rps = 5000;
    options.active = true;
    options.checks = vec!["fuzz".to_string()];
    options.forge_root = Some(forge_root());
    options.fuzz = true;
    options.token = Some("a-token".to_string());
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
        .any(|f| f.check == "fuzz" && f.severity >= severity && f.detail.contains(needle))
}

#[quench_test::test]
async fn a_flawed_service_is_caught_from_its_openapi_alone() {
    let port = serve(true).await;
    let (findings, requests) = scan::run(options(port)).await.unwrap();
    let shown = report(&findings);
    assert!(
        requests > 60,
        "suspiciously few requests: {requests}\n{shown}"
    );
    assert!(
        has(&findings, Severity::Medium, "answered 500 to"),
        "a 500 was missed:\n{shown}"
    );
    assert!(
        has(&findings, Severity::High, "syntax error")
            || has(&findings, Severity::High, "PostgreSQL"),
        "a leaked database error was missed:\n{shown}"
    );
    assert!(
        has(&findings, Severity::Medium, "panic"),
        "a leaked panic was missed:\n{shown}"
    );
    assert!(
        has(&findings, Severity::High, "a field it must not"),
        "a client-settable is_admin was missed:\n{shown}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.path == "/api/v1/search" && f.severity >= Severity::Medium),
        "the query-string crash was missed:\n{shown}"
    );
    assert!(
        has(&findings, Severity::Low, "wrong type"),
        "a lax type check was missed:\n{shown}"
    );
}

#[quench_test::test]
async fn a_strict_service_is_left_alone() {
    let port = serve(false).await;
    let (findings, requests) = scan::run(options(port)).await.unwrap();
    let shown = report(&findings);
    assert!(
        requests > 60,
        "suspiciously few requests: {requests}\n{shown}"
    );
    assert!(
        findings.iter().all(|f| f.severity <= Severity::Info),
        "a strict service was accused:\n{shown}"
    );
}

#[quench_test::test]
async fn without_active_nothing_is_created_or_mutated() {
    let port = serve(true).await;
    let mut opts = options(port);
    opts.active = false;
    let (findings, _) = scan::run(opts).await.unwrap();
    assert!(
        findings.iter().all(|f| f.method != "POST"),
        "a mutating route was fuzzed without --active:\n{}",
        report(&findings)
    );
    assert!(
        findings.iter().any(|f| f.detail.contains("--active")),
        "{}",
        report(&findings)
    );
}

const BIND_SPEC: &str = r#"
[[service]]
name = "notes"
reader = "notes:read"
writer = "notes:read notes:write"

[[bind]]
service = "notes"
param = "kind"
value = "widget"
"#;

#[quench_test::test]
async fn a_path_parameter_is_fuzzed_too() {
    let port = serve(true).await;
    let (findings, _) = scan::run(options(port)).await.unwrap();
    let shown = report(&findings);
    assert!(
        findings
            .iter()
            .any(|f| f.path == "/api/v1/items/{id}" && f.severity >= Severity::Medium),
        "a crash on a hostile id was missed:\n{shown}"
    );
}

#[quench_test::test]
async fn an_id_is_found_in_the_list_and_one_the_spec_names_is_used() {
    let port = serve(false).await;
    let mut opts = options(port);
    opts.authz = Some(forge_sectest::identity::Spec::parse(BIND_SPEC).unwrap());
    let (findings, _) = scan::run(opts).await.unwrap();
    let shown = report(&findings);
    assert!(
        findings.iter().all(|f| !f.detail.contains("names nothing")),
        "an id the list holds, and one the spec gives, are real:\n{shown}"
    );
}

#[quench_test::test]
async fn an_id_that_cannot_be_found_is_fuzzed_anyway_and_said_so() {
    let port = serve(true).await;
    // Nothing stored, nothing bound: the `things` id names nothing.
    let (findings, _) = scan::run(options(port)).await.unwrap();
    let shown = report(&findings);
    assert!(
        has(&findings, Severity::Info, "names nothing")
            && has(&findings, Severity::Info, "/api/v1/things/{kind}"),
        "the operation fuzzed against a stand-in id must be listed:\n{shown}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.path == "/api/v1/things/{kind}" && f.severity >= Severity::Medium),
        "the stand-in id still gets its path parameter fuzzed:\n{shown}"
    );
}

#[quench_test::test]
async fn asking_for_fuzz_without_a_checkout_is_an_error() {
    let port = serve(false).await;
    let mut opts = options(port);
    opts.forge_root = None;
    let error = scan::run(opts).await.unwrap_err();
    assert!(error.contains("--forge"), "{error}");
}

#[quench_test::test]
fn the_document_is_read_into_operations_with_their_references_resolved() {
    let ops = openapi::parse(DOCUMENT).unwrap();
    assert_eq!(ops.len(), 5);
    let create = ops.iter().find(|o| o.id == "createItem").unwrap();
    assert_eq!(create.method, "POST");
    let schema = create.body.as_ref().unwrap();
    assert_eq!(openapi::required(schema), vec!["name".to_string()]);
    assert_eq!(
        openapi::type_of(schema.get("properties").unwrap().get("count").unwrap()),
        "integer"
    );
    assert!(create.example.is_some());
    let get = ops.iter().find(|o| o.id == "getItem").unwrap();
    assert_eq!(get.params.len(), 1);
    assert_eq!(get.params[0].location, "path");
    assert_eq!(
        ops.iter().find(|o| o.id == "search").unwrap().params.len(),
        2
    );
}

#[quench_test::test]
fn a_valid_sample_is_built_from_a_schema() {
    let ops = openapi::parse(DOCUMENT).unwrap();
    let create = ops.iter().find(|o| o.id == "createItem").unwrap();
    let schema = create.body.as_ref().unwrap();
    let value = openapi::sample(schema);
    assert_eq!(value.get("name").and_then(Value::as_str), Some("sectest"));
    // Only what the schema requires: the rest is optional and may be refused for reasons of its own.
    assert!(value.get("count").is_none());
}

quench_test::main!();
