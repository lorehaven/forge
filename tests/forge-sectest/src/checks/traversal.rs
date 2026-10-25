//! Path traversal and sensitive-file exposure.
//!
//! Every path parameter of every GET route is replaced, one at a time, with encodings of `../` that
//! different layers decode differently (raw, single-encoded, double-encoded, overlong UTF-8,
//! backslash). The router, the handler and the filesystem layer must all agree it is just a name.
//! A second pass asks for files that should never be served at all.

use super::leak_marker;
use crate::census::{Piece, render};
use crate::finding::{Finding, Severity};
use crate::scan::Ctx;
use crate::wire::{Probe, Target};

const PAYLOADS: [&str; 12] = [
    "../../../../../../etc/passwd",
    "..%2f..%2f..%2f..%2f..%2fetc%2fpasswd",
    "%2e%2e/%2e%2e/%2e%2e/%2e%2e/etc/passwd",
    "....//....//....//....//etc/passwd",
    "%252e%252e%252f%252e%252e%252fetc%252fpasswd",
    "..\\..\\..\\..\\windows\\win.ini",
    "/etc/passwd",
    "%00",
    "..%c0%af..%c0%af..%c0%afetc/passwd",
    "..;/..;/etc/passwd",
    "a/../../../etc/passwd",
    "%2e%2e%5c%2e%2e%5cetc%5cpasswd",
];

/// Paths that must never answer 200 with real content.
const EXPOSED: [(&str, &str); 9] = [
    ("/.git/HEAD", "ref:"),
    ("/.git/config", "[core]"),
    ("/.env", "="),
    ("/.DS_Store", "Bud1"),
    ("/../../../../../../etc/passwd", "root:"),
    ("/%2e%2e/%2e%2e/%2e%2e/%2e%2e/etc/passwd", "root:"),
    ("//etc/passwd", "root:"),
    ("/static/..%2f..%2f..%2f..%2fetc/passwd", "root:"),
    ("/actuator/env", "propertySources"),
];

pub async fn run(ctx: &Ctx, target: &Target) -> Vec<Finding> {
    let mut findings = Vec::new();

    for route in ctx.routes_of(target).filter(|r| r.method == "GET" && r.has_params()) {
        let pieces = route.pieces();
        for (index, piece) in pieces.iter().enumerate() {
            if !matches!(piece, Piece::Param { .. }) {
                continue;
            }
            for payload in PAYLOADS {
                let path = render(&pieces, Some((index, payload)));
                let Some(reply) = ctx.send(target, &ctx.authed(Probe::new("GET", &path))).await else {
                    continue;
                };
                if let Some((severity, what)) = leak_marker(&reply.body)
                    && severity == Severity::Critical
                {
                    findings.push(Finding::new(
                        severity,
                        "traversal",
                        &target.name,
                        "GET",
                        &route.pattern,
                        format!("PATH TRAVERSAL: {what} returned for parameter value {payload:?}"),
                    ));
                } else if reply.status >= 500 {
                    findings.push(Finding::new(
                        Severity::Medium,
                        "traversal",
                        &target.name,
                        "GET",
                        &route.pattern,
                        format!("{} for parameter value {payload:?} (unhandled input)", reply.status),
                    ));
                }
            }
        }
    }

    for (path, marker) in EXPOSED {
        let Some(reply) = ctx.send(target, &Probe::new("GET", path)).await else {
            continue;
        };
        if reply.status == 200 && reply.body.contains(marker) && !reply.is_html() {
            findings.push(Finding::new(
                Severity::High,
                "traversal",
                &target.name,
                "GET",
                path,
                "sensitive file is served to anonymous callers",
            ));
        }
    }
    findings
}
