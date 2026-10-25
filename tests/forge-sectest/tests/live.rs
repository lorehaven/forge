//! The live scan of a running estate, one case per check, in the same report as the gate.
//!
//!     FORGE_SECTEST_TARGETS="warehouse=http://warehouse.forge.svc:8080 gatehouse=http://..."  (required)
//!     FORGE_SECTEST_TOKEN        a valid bearer token, to test behind the auth wall
//!     FORGE_SECTEST_ACTIVE=1     also send mutating verbs and credential bursts (staging only!)
//!     FORGE_SECTEST_FAIL_ON      lowest severity that fails a case: info|low|medium|high|critical (default high)
//!     FORGE_SECTEST_BASELINE     reviewed anonymous routes (default tests/forge-sectest/public-routes.txt if present)
//!     FORGE_SECTEST_RPS          request rate limit (default 20)
//!
//! Every finding, not only the failing ones, is written to `target/security-findings.json` for the job
//! to keep. With no targets configured every case FAILS: a nightly that scanned nothing is not a pass.

#![allow(clippy::wildcard_imports)]

use forge_sectest::census;
use forge_sectest::finding::{Finding, Severity, report_json};
use forge_sectest::scan::{self, Options};
use forge_sectest::wire::Target;
use std::collections::HashSet;
use std::sync::OnceLock;

struct Scan {
    findings: Vec<Finding>,
    requests: u64,
    fail_on: Severity,
    active: bool,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn run_scan() -> Result<Scan, String> {
    let spec = env("FORGE_SECTEST_TARGETS").ok_or(
        "FORGE_SECTEST_TARGETS is not set: there is nothing to scan, which must not read as a pass",
    )?;
    let root = forge_sectest::gate::workspace_root();

    let mut targets = Vec::new();
    for entry in spec.split([' ', ',', '\n']).filter(|e| !e.is_empty()) {
        let (name, url) = entry
            .split_once('=')
            .ok_or_else(|| format!("{entry}: expected NAME=URL"))?;
        let mut target = Target::parse(name, url)?;
        target.insecure = env("FORGE_SECTEST_INSECURE").is_some();
        targets.push(target);
    }

    let extra = census::quench_route_sources(&root);
    let routes = census::scan_forge(&root, &extra);
    if routes.len() < 100 {
        return Err(format!(
            "the route census found only {} routes under {}",
            routes.len(),
            root.display()
        ));
    }

    let mut options = Options::new(targets, routes);
    options.token = env("FORGE_SECTEST_TOKEN");
    options.active = env("FORGE_SECTEST_ACTIVE").is_some();
    options.allow_remote = env("FORGE_SECTEST_ALLOW_REMOTE").is_some();
    if let Some(rps) = env("FORGE_SECTEST_RPS") {
        options.rps = rps
            .parse()
            .map_err(|_| "FORGE_SECTEST_RPS: not a number".to_string())?;
    }
    let baseline = env("FORGE_SECTEST_BASELINE").map_or_else(
        || root.join("tests/forge-sectest/public-routes.txt"),
        std::path::PathBuf::from,
    );
    if let Ok(text) = std::fs::read_to_string(&baseline) {
        options.baseline = Some(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string)
                .collect::<HashSet<_>>(),
        );
    }
    let fail_on = match env("FORGE_SECTEST_FAIL_ON") {
        Some(level) => Severity::parse(&level)
            .ok_or_else(|| format!("FORGE_SECTEST_FAIL_ON: unknown level {level:?}"))?,
        None => Severity::High,
    };
    let active = options.active;

    // The scanner is async; the cases run inside a runtime of their own, so it gets a thread and runtime
    // of its own too.
    let (findings, requests) = std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?
            .block_on(scan::run(options))
    })
    .join()
    .map_err(|_| "the scanner panicked".to_string())??;

    let findings_path = root.join("target/security-findings.json");
    let _ = std::fs::create_dir_all(findings_path.parent().unwrap_or(&root));
    let _ = std::fs::write(&findings_path, report_json(&findings));

    Ok(Scan {
        findings,
        requests,
        fail_on,
        active,
    })
}

fn scan() -> &'static Result<Scan, String> {
    static SCAN: OnceLock<Result<Scan, String>> = OnceLock::new();
    SCAN.get_or_init(run_scan)
}

fn check(name: &str) {
    let scan = match scan() {
        Ok(scan) => scan,
        Err(error) => panic!("the live scan did not run: {error}"),
    };
    let serious: Vec<String> = scan
        .findings
        .iter()
        .filter(|f| f.check == name && f.severity >= scan.fail_on)
        .take(30)
        .map(ToString::to_string)
        .collect();
    assert!(
        serious.is_empty(),
        "{} finding(s) at or above {} in `{name}`:\n{}",
        serious.len(),
        scan.fail_on,
        serious.join("\n")
    );
}

#[quench_test::test]
fn live_scan_actually_ran() {
    let scan = match scan() {
        Ok(scan) => scan,
        Err(error) => panic!("the live scan did not run: {error}"),
    };
    assert!(
        scan.requests >= 200,
        "only {} requests were made: the scan covered almost nothing",
        scan.requests
    );
    let broken: Vec<String> = scan
        .findings
        .iter()
        .filter(|f| f.check == "scan")
        .map(ToString::to_string)
        .collect();
    assert!(
        broken.is_empty(),
        "the scan itself was unreliable:\n{}",
        broken.join("\n")
    );
    println!(
        "{} requests, {} findings (active: {})",
        scan.requests,
        scan.findings.len(),
        scan.active
    );
}

#[quench_test::test]
fn live_auth_anonymous_routes_match_the_reviewed_baseline() {
    check("auth");
}
#[quench_test::test]
fn live_tokens_forged_credentials_are_refused() {
    check("tokens");
}
#[quench_test::test]
fn live_traversal_paths_cannot_escape() {
    check("traversal");
}
#[quench_test::test]
fn live_headers_responses_are_hardened() {
    check("headers");
}
#[quench_test::test]
fn live_cors_hostile_origins_are_refused() {
    check("cors");
}
#[quench_test::test]
fn live_injection_hostile_input_is_contained() {
    check("injection");
}
#[quench_test::test]
fn live_protocol_smuggling_is_refused() {
    check("protocol");
}
#[quench_test::test]
fn live_throttle_credential_guessing_is_slowed() {
    check("throttle");
}

quench_test::main!();
