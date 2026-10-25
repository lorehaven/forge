//! Response hardening: security headers, cookie flags, information disclosure, `TRACE`, and Host
//! header handling. Sampled from a few unauthenticated responses per service; one finding per issue.

use super::leak_marker;
use crate::finding::{Finding, Severity};
use crate::scan::Ctx;
use crate::wire::{Probe, Reply, Target};
use std::collections::BTreeSet;

pub async fn run(ctx: &Ctx, target: &Target) -> Vec<Finding> {
    let mut paths: Vec<String> = vec!["/".to_string()];
    paths.extend(
        ctx.routes_of(target)
            .filter(|r| r.method == "GET" && !r.has_params())
            .map(|r| r.pattern.clone())
            .take(8),
    );

    let mut issues: BTreeSet<(Severity, String)> = BTreeSet::new();
    for path in &paths {
        let Some(reply) = ctx.send(target, &Probe::new("GET", path)).await else {
            continue;
        };
        inspect(&reply, target, &mut issues);
    }

    // A path that cannot exist: the error page is what an attacker sees first.
    if let Some(reply) = ctx.send(target, &Probe::new("GET", "/sectest-no-such-route-7f3a")).await
        && let Some((severity, what)) = leak_marker(&reply.body)
    {
        issues.insert((severity, format!("the 404 page leaks {what}")));
    }

    if let Some(reply) = ctx.send(target, &Probe::new("TRACE", "/")).await
        && (200..300).contains(&reply.status)
    {
        issues.insert((Severity::Low, "TRACE is enabled and answers 2xx".to_string()));
    }

    // Host-header injection: a reflected Host poisons password-reset links and caches.
    let evil = "sectest-evil.invalid";
    for header in ["Host", "X-Forwarded-Host"] {
        let probe = Probe::new("GET", "/").header(header, evil);
        if let Some(reply) = ctx.send(target, &probe).await {
            let reflected = reply.body.contains(evil)
                || reply.header("location").is_some_and(|l| l.contains(evil));
            if reflected && reply.status < 400 {
                issues.insert((Severity::Medium, format!("{header} is reflected into the response (host-header injection)")));
            }
        }
    }

    issues
        .into_iter()
        .map(|(severity, detail)| Finding::new(severity, "headers", &target.name, "GET", "(sampled)", detail))
        .collect()
}

fn inspect(reply: &Reply, target: &Target, issues: &mut BTreeSet<(Severity, String)>) {
    if reply.header("x-content-type-options").is_none_or(|v| !v.eq_ignore_ascii_case("nosniff")) {
        issues.insert((Severity::Low, "missing X-Content-Type-Options: nosniff".to_string()));
    }
    if reply.is_html() {
        let csp = reply.header("content-security-policy").unwrap_or("").to_ascii_lowercase();
        if reply.header("x-frame-options").is_none() && !csp.contains("frame-ancestors") {
            issues.insert((Severity::Low, "HTML responses can be framed (no X-Frame-Options / frame-ancestors): clickjacking".to_string()));
        }
        if csp.is_empty() {
            issues.insert((Severity::Info, "HTML responses carry no Content-Security-Policy".to_string()));
        }
    }
    if target.tls && reply.header("strict-transport-security").is_none() {
        issues.insert((Severity::Low, "HTTPS without Strict-Transport-Security".to_string()));
    }
    if let Some(server) = reply.header("server")
        && server.chars().any(|c| c.is_ascii_digit())
    {
        issues.insert((Severity::Low, format!("Server header discloses a version: {server:?}")));
    }
    if let Some(powered) = reply.header("x-powered-by") {
        issues.insert((Severity::Low, format!("X-Powered-By discloses the stack: {powered:?}")));
    }
    for cookie in reply.all("set-cookie") {
        let lower = cookie.to_ascii_lowercase();
        let name = cookie.split('=').next().unwrap_or("?");
        if !lower.contains("httponly") {
            issues.insert((Severity::Medium, format!("cookie {name:?} lacks HttpOnly")));
        }
        if !lower.contains("samesite") {
            issues.insert((Severity::Medium, format!("cookie {name:?} lacks SameSite")));
        }
        if target.tls && !lower.contains("secure") {
            issues.insert((Severity::Medium, format!("cookie {name:?} lacks Secure over HTTPS")));
        }
    }
}
