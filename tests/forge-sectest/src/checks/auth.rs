//! The auth matrix: which routes answer a caller who presents nothing at all?
//!
//! Every route is requested with no credentials. 401/403 means the auth wall held; anything else is
//! a route an anonymous caller can reach, which must be on the reviewed baseline or it is reported.
//! The baseline is what turns "someone forgot to wrap the new router in `Auth`" from a silent hole
//! into a failing check.

use crate::census::Route;
use crate::finding::{Finding, Severity};
use crate::scan::Ctx;
use crate::wire::{Probe, Reply, Target};

#[derive(Debug)]
pub struct Outcome {
    pub findings: Vec<Finding>,
    /// Routes that answered 401/403 (or redirected to a login), for the forged-credential check.
    pub protected: Vec<Route>,
}

const LOGIN_HINTS: [&str; 6] = ["login", "authorize", "sso", "signin", "oauth", "/auth"];

#[derive(Debug, PartialEq, Eq)]
enum Class {
    Protected,
    /// 400/422: the request was rejected for its parameters, so whether auth guards the route is unknown.
    Inconclusive,
    NotRouted,
    Public,
    ServerError,
}

/// A redirect whose target is a login or authorisation page: the page is behind auth.
pub(crate) fn redirects_to_login(reply: &Reply) -> bool {
    matches!(reply.status, 301 | 302 | 303 | 307 | 308) && {
        let location = reply.header("location").unwrap_or("").to_ascii_lowercase();
        LOGIN_HINTS.iter().any(|hint| location.contains(hint))
    }
}

fn classify(reply: &Reply) -> Class {
    match reply.status {
        401 | 403 => Class::Protected,
        404 | 405 => Class::NotRouted,
        400 | 422 => Class::Inconclusive,
        301 | 302 | 303 | 307 | 308 => {
            if redirects_to_login(reply) {
                Class::Protected
            } else {
                Class::Public
            }
        }
        500..=599 => Class::ServerError,
        _ => Class::Public,
    }
}

pub async fn run(ctx: &Ctx, target: &Target) -> Outcome {
    let mut findings = Vec::new();
    let mut protected = Vec::new();
    let (mut skipped, mut not_routed, mut total) = (0usize, 0usize, 0usize);
    let mut inconclusive: Vec<String> = Vec::new();

    for route in ctx.routes_of(target) {
        if route.is_mutating() && !ctx.opts.active {
            skipped += 1;
            continue;
        }
        total += 1;
        let mut probe = Probe::new(&route.method, &route.sample_path());
        if matches!(route.method.as_str(), "POST" | "PUT" | "PATCH") {
            probe = probe.json("{}");
        }
        let Some(reply) = ctx.send(target, &probe).await else {
            continue;
        };
        match classify(&reply) {
            Class::Protected => protected.push(route.clone()),
            Class::NotRouted => not_routed += 1,
            Class::Inconclusive => inconclusive.push(format!("{} {}", route.method, route.pattern)),
            Class::Public | Class::ServerError => {
                let known = ctx
                    .opts
                    .baseline
                    .as_ref()
                    .is_some_and(|b| b.contains(&route.key()));
                if known {
                    continue;
                }
                let severity = if route.is_mutating() {
                    Severity::High
                } else {
                    Severity::Medium
                };
                findings.push(Finding::new(
                    severity,
                    "auth",
                    &target.name,
                    &route.method,
                    &route.pattern,
                    format!(
                        "answered {} to an anonymous request and is not in the public-route baseline ({})",
                        reply.status, route.source
                    ),
                ));
            }
        }
    }

    if !inconclusive.is_empty() {
        let shown = inconclusive
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let more = inconclusive.len().saturating_sub(8);
        findings.push(Finding::new(
            Severity::Info,
            "auth",
            &target.name,
            "-",
            "-",
            format!(
                "{} routes rejected the probe as malformed (400/422), so whether auth guards them is unknown - give them valid parameters to test: {shown}{}",
                inconclusive.len(),
                if more > 0 { format!(" and {more} more") } else { String::new() }
            ),
        ));
    }
    if skipped > 0 {
        findings.push(Finding::new(
            Severity::Info,
            "auth",
            &target.name,
            "-",
            "-",
            format!("{skipped} mutating routes were not probed; pass --active to include them"),
        ));
    }
    if total > 5 && not_routed * 10 > total * 8 {
        findings.push(Finding::new(
            Severity::Medium,
            "auth",
            &target.name,
            "-",
            "-",
            format!(
                "{not_routed} of {total} routes answered 404/405: wrong target URL or --base-path? The auth matrix is not meaningful until routes resolve"
            ),
        ));
    }
    Outcome {
        findings,
        protected,
    }
}
