//! Proves the scanner works: it must flag a deliberately vulnerable app on every axis it claims to
//! cover, and stay quiet on a hardened one. A scanner that has never been shown a vulnerability it
//! detects proves nothing when it reports a clean bill of health.

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
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn decode(text: &str) -> String {
    text.replace("%3C", "<")
        .replace("%3E", ">")
        .replace("%2F", "/")
}

/// Everything wrong that an application can be: no auth, accepts any token, traversal, crashes,
/// reflected XSS, credentialed wildcard CORS, no hardening headers.
struct Vulnerable;

#[async_trait]
impl Endpoint for Vulnerable {
    async fn call(&self, req: Request) -> Response {
        let path = req.uri().path().to_string();
        let origin = req.header("origin").map(str::to_string);
        let authorization = req.header("authorization").map(str::to_string);
        let mut response = if path == "/api/data" {
            // Treats any non-empty Authorization header as valid.
            match authorization {
                Some(value) if value.len() > 7 => Response::text(StatusCode::OK, "secret data"),
                _ => Response::text(StatusCode::UNAUTHORIZED, "no"),
            }
        } else if path.starts_with("/files/") {
            if path.contains("etc/passwd") || path.contains("etc%2fpasswd") {
                Response::text(StatusCode::OK, "root:x:0:0:root:/root:/bin/bash\n")
            } else {
                Response::text(StatusCode::NOT_FOUND, "nope")
            }
        } else if path == "/boom" {
            Response::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "thread 'main' panicked at src/main.rs:1:1",
            )
        } else if path == "/search" {
            let query = decode(req.uri().query().unwrap_or(""));
            Response::html(StatusCode::OK, format!("<html>results for {query}</html>"))
        } else if path == "/admin/users" || path == "/health" {
            Response::text(StatusCode::OK, "[]")
        } else if req.method().as_str() == "DELETE" && path.starts_with("/admin/users/") {
            Response::text(StatusCode::OK, "deleted")
        } else {
            Response::not_found()
        };
        if let Some(origin) = origin {
            response = response
                .header("access-control-allow-origin", origin)
                .header("access-control-allow-credentials", "true");
        }
        response.header("server", "VulnServer/1.2.3")
    }
}

/// The same surface, done right: everything needs the one valid token, nothing leaks.
struct Hardened;

#[async_trait]
impl Endpoint for Hardened {
    async fn call(&self, req: Request) -> Response {
        let known = matches!(
            req.uri().path(),
            "/api/data" | "/boom" | "/search" | "/admin/users" | "/health"
        ) || req.uri().path().starts_with("/files/")
            || req.uri().path().starts_with("/admin/users/");
        let response = if !known {
            Response::not_found()
        } else if req.header("authorization") == Some("Bearer the-one-valid-token") {
            Response::text(StatusCode::OK, "ok")
        } else {
            Response::text(StatusCode::UNAUTHORIZED, "unauthorized")
        };
        response.header("x-content-type-options", "nosniff")
    }
}

/// A server-rendered UI: pages send callers without a session to the login page, and one page
/// wants parameters before it looks at anything.
struct Pages;

#[async_trait]
impl Endpoint for Pages {
    async fn call(&self, req: Request) -> Response {
        let response = match req.uri().path() {
            "/ui/modal" if req.uri().query().is_none() => {
                Response::text(StatusCode::BAD_REQUEST, "missing field `id`")
            }
            "/ui/home" | "/ui/modal" => {
                Response::new(StatusCode::FOUND).header("location", "/ui/login")
            }
            _ => Response::not_found(),
        };
        response.header("x-content-type-options", "nosniff")
    }
}

async fn serve(app: Arc<dyn Endpoint>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    tokio::spawn(serve_http_on(
        app,
        container,
        listener,
        std::future::pending::<()>(),
    ));
    port
}

/// A server whose parser is lenient: it answers every request-looking line it sees, so a smuggled
/// request behind a CL+TE message gets its own response - exactly the desync the scanner must catch.
async fn serve_desync_prone() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut data = Vec::new();
                let mut buf = vec![0u8; 65536];
                while let Ok(Ok(n)) =
                    tokio::time::timeout(Duration::from_millis(300), stream.read(&mut buf)).await
                {
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..n]);
                }
                let requests = String::from_utf8_lossy(&data)
                    .matches(" HTTP/1.")
                    .count()
                    .max(1);
                for _ in 0..requests {
                    let _ = stream
                        .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                        .await;
                }
                let _ = stream.shutdown().await;
            });
        }
    });
    port
}

fn routes(service: &str) -> Vec<Route> {
    [
        ("GET", "/health"),
        ("GET", "/admin/users"),
        ("DELETE", "/admin/users/{user_id}"),
        ("GET", "/api/data"),
        ("GET", "/files/{name:.+}"),
        ("GET", "/boom"),
        ("GET", "/search"),
    ]
    .iter()
    .map(|(method, pattern)| Route::new(service, method, pattern))
    .collect()
}

fn options(service: &str, port: u16) -> Options {
    let target = Target::parse(service, &format!("http://127.0.0.1:{port}")).unwrap();
    let mut options = Options::new(vec![target], routes(service));
    options.rps = 2000;
    options
}

fn has(findings: &[Finding], check: &str, severity: Severity, needle: &str) -> bool {
    findings
        .iter()
        .any(|f| f.check == check && f.severity >= severity && f.detail.contains(needle))
}

#[quench_test::test]
async fn every_check_flags_a_vulnerable_application() {
    let port = serve(Arc::new(Vulnerable)).await;
    let mut opts = options("vuln", port);
    opts.active = true;
    let (findings, requests) = scan::run(opts).await.unwrap();
    let report = findings
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(requests > 50, "suspiciously few requests: {requests}");

    assert!(
        has(&findings, "auth", Severity::High, "anonymous"),
        "anonymous DELETE not flagged:\n{report}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.check == "auth" && f.path == "/admin/users"),
        "public /admin/users not flagged:\n{report}"
    );
    assert!(
        has(&findings, "tokens", Severity::Critical, "BYPASS"),
        "forged token accepted but not flagged:\n{report}"
    );
    assert!(
        has(&findings, "traversal", Severity::Critical, "PATH TRAVERSAL"),
        "traversal missed:\n{report}"
    );
    assert!(
        has(
            &findings,
            "injection",
            Severity::Medium,
            "crashed the handler"
        ),
        "5xx missed:\n{report}"
    );
    assert!(
        has(&findings, "injection", Severity::Medium, "leaks"),
        "stack-trace leak missed:\n{report}"
    );
    assert!(
        has(&findings, "injection", Severity::High, "REFLECTED XSS"),
        "XSS missed:\n{report}"
    );
    assert!(
        has(&findings, "cors", Severity::High, "any site can read"),
        "credentialed CORS missed:\n{report}"
    );
    assert!(
        has(&findings, "headers", Severity::Low, "nosniff"),
        "missing nosniff missed:\n{report}"
    );
    assert!(
        has(&findings, "headers", Severity::Low, "VulnServer"),
        "Server version disclosure missed:\n{report}"
    );
}

#[quench_test::test]
async fn login_redirects_are_protection_and_rejected_probes_are_inconclusive() {
    let port = serve(Arc::new(Pages)).await;
    let service = "pages";
    let target = Target::parse(service, &format!("http://127.0.0.1:{port}")).unwrap();
    let routes = vec![
        Route::new(service, "GET", "/ui/home"),
        Route::new(service, "GET", "/ui/modal"),
    ];
    let mut opts = Options::new(vec![target], routes);
    opts.rps = 2000;
    let (findings, _) = scan::run(opts).await.unwrap();
    let report = findings
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        findings.iter().all(|f| f.check != "tokens"),
        "a redirect to login was reported as a failed denial:\n{report}"
    );
    assert!(
        findings.iter().all(|f| f.severity <= Severity::Info),
        "nothing here is a finding above info:\n{report}"
    );
    assert!(
        has(&findings, "auth", Severity::Info, "/ui/modal"),
        "the unprobed route should still be mentioned:\n{report}"
    );
}

#[quench_test::test]
async fn a_hardened_application_produces_nothing_serious() {
    let port = serve(Arc::new(Hardened)).await;
    let mut opts = options("hard", port);
    opts.active = true;
    let (findings, requests) = scan::run(opts).await.unwrap();
    let serious: Vec<String> = findings
        .iter()
        .filter(|f| f.severity > Severity::Low)
        .map(ToString::to_string)
        .collect();
    assert!(requests > 50, "suspiciously few requests: {requests}");
    assert!(
        serious.is_empty(),
        "false positives on a hardened app:\n{}",
        serious.join("\n")
    );
}

#[quench_test::test]
async fn the_real_quench_http_server_resists_every_protocol_probe() {
    // Not the app - the framework underneath it. If quench-proto ever stops rejecting smuggling
    // shapes, this is the test that fails.
    let port = serve(Arc::new(Hardened)).await;
    let mut opts = options("hard", port);
    opts.checks = vec!["protocol".to_string()];
    let (findings, _) = scan::run(opts).await.unwrap();
    assert!(
        findings.is_empty(),
        "{}",
        findings
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[quench_test::test]
async fn a_desync_prone_server_is_caught_by_the_protocol_check() {
    let port = serve_desync_prone().await;
    let mut opts = options("lenient", port);
    opts.checks = vec!["protocol".to_string()];
    let (findings, _) = scan::run(opts).await.unwrap();
    assert!(
        has(&findings, "protocol", Severity::High, "REQUEST SMUGGLING"),
        "smuggling not detected:\n{}",
        findings
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        has(&findings, "protocol", Severity::Medium, "accepted"),
        "oversized heads accepted but not flagged"
    );
}

#[quench_test::test]
async fn the_baseline_silences_reviewed_public_routes_and_only_those() {
    let port = serve(Arc::new(Vulnerable)).await;
    let mut opts = options("vuln", port);
    opts.checks = vec!["auth".to_string()];
    let (before, _) = scan::run(opts.clone()).await.unwrap();
    assert!(before.iter().any(|f| f.path == "/health"));

    opts.baseline = Some(HashSet::from(["vuln GET /health".to_string()]));
    let (after, _) = scan::run(opts).await.unwrap();
    assert!(
        !after.iter().any(|f| f.path == "/health"),
        "baselined route still reported"
    );
    assert!(
        after.iter().any(|f| f.path == "/admin/users"),
        "baseline must not hide other routes"
    );
}

#[quench_test::test]
async fn mutating_probes_never_run_without_active() {
    let port = serve(Arc::new(Vulnerable)).await;
    let (findings, _) = scan::run(options("vuln", port)).await.unwrap();
    assert!(
        !findings
            .iter()
            .any(|f| f.method == "DELETE" && f.severity > Severity::Info),
        "a DELETE was sent without --active"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Info && f.detail.contains("--active"))
    );
}

#[quench_test::test]
async fn remote_targets_are_refused_without_explicit_consent() {
    let target = Target::parse("x", "https://production.example.com").unwrap();
    let error = scan::run(Options::new(vec![target.clone()], Vec::new()))
        .await
        .unwrap_err();
    assert!(error.contains("--allow-remote"), "{error}");
    for private in [
        "http://127.0.0.1:1",
        "http://localhost:1",
        "http://10.1.2.3",
        "http://192.168.0.5",
        "http://warehouse.forge.svc:8080",
    ] {
        assert!(
            Target::parse("x", private).unwrap().is_private(),
            "{private}"
        );
    }
    for public in [
        "http://8.8.8.8",
        "https://ennor.ddns.net",
        "http://172.32.0.1",
    ] {
        assert!(
            !Target::parse("x", public).unwrap().is_private(),
            "{public}"
        );
    }
}

#[quench_test::test]
async fn an_unreachable_target_is_an_error_not_a_clean_report() {
    // Bind then drop to get a port nothing listens on.
    let port = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let error = scan::run(options("ghost", port)).await.unwrap_err();
    assert!(error.contains("unreachable"), "{error}");
}

quench_test::main!();

#[quench_test::test]
fn a_binary_body_is_not_searched_for_leaks() {
    use forge_sectest::wire::Reply;
    use std::time::Duration;
    let reply = |content_type: &str| Reply {
        status: 200,
        headers: vec![("content-type".to_string(), content_type.to_string())],
        body: "...panicked at...".to_string(),
        elapsed: Duration::ZERO,
    };
    assert!(reply("text/html").leak().is_some());
    assert!(reply("application/json").leak().is_some());
    // The wasm client carries its panic handler's text; it is not a response that leaks.
    assert!(reply("application/wasm").leak().is_none());
    assert!(reply("image/png").leak().is_none());
}
