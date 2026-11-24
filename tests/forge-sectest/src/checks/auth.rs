//! The auth matrix: which routes answer a caller who presents nothing at all?
//!
//! Every route is requested with no credentials. 401/403 means the auth wall held; anything else is
//! a route an anonymous caller can reach, which must be on the reviewed baseline or it is reported.
//! The baseline is what turns "someone forgot to wrap the new router in `Auth`" from a silent hole
//! into a failing check.

use crate::census::Route;
use crate::finding::{Finding, Severity};
use crate::openapi::{self, Operation, sample};
use crate::scan::Ctx;
use crate::wire::{Probe, Reply, Target};
use quench_serde::Value;
use std::collections::HashMap;

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
        // 429: throttled, which says nothing about whether the route is guarded.
        400 | 422 | 429 => Class::Inconclusive,
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

/// What it took to get an answer out of a route that rejected the first probe.
#[derive(Debug, PartialEq, Eq)]
enum Hint {
    None,
    Query(String),
    Body,
}

/// The service's documented operations by `METHOD pattern`, when the documents are at hand.
fn documented(ctx: &Ctx, target: &Target) -> HashMap<String, Operation> {
    let Some(root) = &ctx.opts.forge_root else {
        return HashMap::new();
    };
    let file = root
        .join("docs/services")
        .join(&target.name)
        .join("openapi.yaml");
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| openapi::parse(&text).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|op| (format!("{} {}", op.method, op.path), op))
        .collect()
}

/// The route's path with what its document says is required in the query, filled in with plausible values: a link target
/// (`/ui/verify`) answers 400 to a request with no token, which says nothing about whether it is guarded.
fn required_query(op: &Operation) -> Option<String> {
    let query: Vec<String> = op
        .params
        .iter()
        .filter(|p| p.location == "query" && p.required)
        .map(|p| {
            let value = match sample(&p.schema) {
                Value::String(text) => text,
                Value::Int(n) => n.to_string(),
                _ => "sectest".to_string(),
            };
            format!("{}={}", p.name, value.replace(' ', "%20"))
        })
        .collect();
    (!query.is_empty()).then(|| query.join("&"))
}

/// A route rejected as malformed is asked again with what its document says it needs: the query it requires and, where a scan
/// is allowed to change things (`--active`, a throwaway stack), a body of the right shape. Replaces `reply` with the new answer.
async fn ask_again(
    ctx: &Ctx,
    target: &Target,
    route: &Route,
    op: &Operation,
    reply: &mut Reply,
) -> Hint {
    let query = required_query(op);
    let mut path = route.sample_path();
    if let Some(q) = &query {
        path = format!("{path}?{q}");
    }
    let mut again = Probe::new(&route.method, &path);
    let mut body_given = false;
    if matches!(route.method.as_str(), "POST" | "PUT" | "PATCH") {
        again = match (&op.body, ctx.opts.active) {
            (Some(schema), true) => {
                body_given = true;
                again.json(&quench_serde::json::to_string(&sample(schema)))
            }
            _ => again.json("{}"),
        };
    }
    let Some(next) = ctx.send(target, &again).await else {
        return Hint::None;
    };
    *reply = next;
    match (query, body_given) {
        (_, true) => Hint::Body,
        (Some(q), false) => Hint::Query(q),
        (None, false) => Hint::None,
    }
}

/// Whether a mutating route may be asked anonymously without `--active`: when an empty request cannot be a valid one, so a
/// hole would be answered with a validation error and not with a change. A path id is the sentinel, which names nothing; a
/// required body is sent empty. A route with neither (rotate the keys, collect garbage) could run, so it is left alone.
fn safe_to_probe(route: &Route, op: Option<&Operation>) -> bool {
    route.has_params() || op.is_some_and(|o| o.body_required)
}

/// What the probes could not decide or did not do, so a quiet report is not mistaken for a complete one.
fn notes(
    ctx: &Ctx,
    target: &Target,
    inconclusive: &[String],
    skipped: &[String],
    (not_routed, total): (usize, usize),
) -> Vec<Finding> {
    let mut findings = Vec::new();
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
                "{} routes rejected the probe as malformed (400/422), so whether auth guards them is unknown (asked again with what their document requires; add real values, or run with --active for a body of the right shape): {shown}{}",
                inconclusive.len(),
                if more > 0 { format!(" and {more} more") } else { String::new() }
            ),
        ));
    }
    if !skipped.is_empty() {
        let shown = skipped
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let more = skipped.len().saturating_sub(6);
        let why = if ctx.opts.probes_mutating_of(&target.name) {
            "no path id and no required body, so an empty request could be a valid one; --active includes them"
        } else {
            "pass --active to include them, or --probe-mutating <services> for those an empty request cannot make do anything"
        };
        findings.push(Finding::new(
            Severity::Info,
            "auth",
            &target.name,
            "-",
            "-",
            format!(
                "{} mutating routes were not probed ({why}): {shown}{}",
                skipped.len(),
                if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                }
            ),
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
    findings
}

pub async fn run(ctx: &Ctx, target: &Target) -> Outcome {
    let mut findings = Vec::new();
    let mut protected = Vec::new();
    let (mut not_routed, mut total) = (0usize, 0usize);
    let mut skipped: Vec<String> = Vec::new();
    let mut inconclusive: Vec<String> = Vec::new();
    let ops = documented(ctx, target);

    for route in ctx.routes_of(target) {
        let op = ops.get(&format!("{} {}", route.method, route.pattern));
        // Without `--active` a mutating route is asked only where that cannot do anything, and what it answers does not
        // make it one the later checks (forged tokens) are sent to.
        let read_only = route.is_mutating() && !ctx.opts.active;
        if read_only && !(ctx.opts.probes_mutating_of(&target.name) && safe_to_probe(route, op)) {
            skipped.push(format!("{} {}", route.method, route.pattern));
            continue;
        }
        total += 1;
        let mut probe = Probe::new(&route.method, &route.sample_path());
        if matches!(route.method.as_str(), "POST" | "PUT" | "PATCH") {
            probe = probe.json("{}");
        }
        let Some(mut reply) = ctx.send(target, &probe).await else {
            continue;
        };
        let decided_by = if classify(&reply) == Class::Inconclusive
            && let Some(o) = op
        {
            ask_again(ctx, target, route, o, &mut reply).await
        } else {
            Hint::None
        };
        match classify(&reply) {
            // Later checks (forged tokens, grants) ask a protected route again. One that was only decided with a body of the
            // right shape cannot be asked again without it, so they leave it alone, as before.
            Class::Protected if !read_only && decided_by != Hint::Body => {
                let mut route = route.clone();
                if let Hint::Query(q) = decided_by {
                    route.hint_query = Some(q);
                }
                protected.push(route);
            }
            Class::Protected => {}
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

    findings.extend(notes(
        ctx,
        target,
        &inconclusive,
        &skipped,
        (not_routed, total),
    ));
    Outcome {
        findings,
        protected,
    }
}
