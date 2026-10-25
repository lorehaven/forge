//! Runs the selected checks against every target.

use crate::census::Route;
use crate::checks;
use crate::finding::Finding;
use crate::wire::{self, Probe, Reply, Target};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub const ALL_CHECKS: [&str; 8] = [
    "auth",
    "tokens",
    "traversal",
    "headers",
    "cors",
    "injection",
    "protocol",
    "throttle",
];

#[derive(Debug, Clone)]
pub struct Options {
    pub targets: Vec<Target>,
    pub routes: Vec<Route>,
    /// A valid credential, to test authorisation and parsing behind the auth wall.
    pub token: Option<String>,
    /// Allow mutating verbs, brute-force-shaped bursts and other state-changing probes.
    pub active: bool,
    /// Also run the (slow, 45s+) slowloris hold test.
    pub slow: bool,
    pub allow_remote: bool,
    pub rps: u32,
    /// Routes already known to be public, as `Route::key` strings; they are not reported.
    pub baseline: Option<HashSet<String>>,
    /// Empty means every check.
    pub checks: Vec<String>,
}

impl Options {
    #[must_use]
    pub const fn new(targets: Vec<Target>, routes: Vec<Route>) -> Self {
        Self {
            targets,
            routes,
            token: None,
            active: false,
            slow: false,
            allow_remote: false,
            rps: 20,
            baseline: None,
            checks: Vec::new(),
        }
    }

    #[must_use]
    pub fn wants(&self, check: &str) -> bool {
        self.checks.is_empty() || self.checks.iter().any(|c| c == check)
    }
}

#[derive(Debug)]
pub struct Ctx {
    pub opts: Options,
    next_slot: Mutex<Instant>,
    sent: AtomicU64,
    failed: AtomicU64,
}

impl Ctx {
    #[must_use]
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            next_slot: Mutex::new(Instant::now()),
            sent: AtomicU64::new(0),
            failed: AtomicU64::new(0),
        }
    }

    #[must_use]
    pub fn requests(&self) -> u64 {
        self.sent.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn failures(&self) -> u64 {
        self.failed.load(Ordering::Relaxed)
    }

    /// Rate-limited send. A transport failure counts as `None` (and is tallied), never a pass.
    pub async fn send(&self, target: &Target, probe: &Probe) -> Option<Reply> {
        let gap = Duration::from_secs(1) / self.opts.rps.max(1);
        let wait = {
            let mut slot = self.next_slot.lock().await;
            let now = Instant::now();
            let at = (*slot).max(now);
            *slot = at + gap;
            drop(slot);
            at - now
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
        self.send_unthrottled(target, probe).await
    }

    /// For checks whose point is speed (rate-limit detection); only reachable under `--active`.
    pub async fn send_unthrottled(&self, target: &Target, probe: &Probe) -> Option<Reply> {
        self.sent.fetch_add(1, Ordering::Relaxed);
        let reply = wire::send(target, probe).await.ok();
        if reply.is_none() {
            self.failed.fetch_add(1, Ordering::Relaxed);
        }
        reply
    }

    pub fn routes_of<'a>(&'a self, target: &'a Target) -> impl Iterator<Item = &'a Route> {
        self.opts
            .routes
            .iter()
            .filter(move |r| r.service == target.name)
    }

    /// The credential header(s) for authenticated probing, if a token was supplied.
    #[must_use]
    pub fn authed(&self, probe: Probe) -> Probe {
        let Some(token) = &self.opts.token else {
            return probe;
        };
        probe.bearer(token)
    }
}

/// Runs everything selected and returns all findings, most severe first.
///
/// # Errors
///
/// Refuses non-private targets without `allow_remote`, and fails if a target cannot be reached at
/// all (an unreachable target must never look "clean").
pub async fn run(opts: Options) -> Result<(Vec<Finding>, u64), String> {
    if opts.targets.is_empty() {
        return Err("no targets: pass --target name=URL".to_string());
    }
    for target in &opts.targets {
        if !opts.allow_remote && !target.is_private() {
            return Err(format!(
                "{} ({}) is not a loopback/private address; pass --allow-remote only for systems you own and may test",
                target.name, target.host
            ));
        }
    }
    let ctx = Ctx::new(opts);
    let mut findings = Vec::new();
    for target in &ctx.opts.targets {
        // Preflight: if we cannot even talk to it, every check below would "pass" vacuously.
        wire::send(target, &Probe::new("GET", "/"))
            .await
            .map_err(|e| format!("{}: unreachable: {e}", target.name))?;
        findings.extend(run_target(&ctx, target).await);
    }
    if ctx.failures() * 2 > ctx.requests() {
        findings.push(Finding::new(
            crate::finding::Severity::High,
            "scan",
            "-",
            "-",
            "-",
            format!(
                "{} of {} requests failed at the transport level: results are unreliable",
                ctx.failures(),
                ctx.requests()
            ),
        ));
    }
    findings.sort_by_key(|f| std::cmp::Reverse(f.severity));
    Ok((findings, ctx.requests()))
}

async fn run_target(ctx: &Ctx, target: &Target) -> Vec<Finding> {
    let mut findings = Vec::new();
    let protected = if ctx.opts.wants("auth") || ctx.opts.wants("tokens") {
        let outcome = checks::auth::run(ctx, target).await;
        if ctx.opts.wants("auth") {
            findings.extend(outcome.findings);
        }
        outcome.protected
    } else {
        Vec::new()
    };
    if ctx.opts.wants("tokens") {
        findings.extend(checks::tokens::run(ctx, target, &protected).await);
    }
    if ctx.opts.wants("traversal") {
        findings.extend(checks::traversal::run(ctx, target).await);
    }
    if ctx.opts.wants("headers") {
        findings.extend(checks::headers::run(ctx, target).await);
    }
    if ctx.opts.wants("cors") {
        findings.extend(checks::cors::run(ctx, target).await);
    }
    if ctx.opts.wants("injection") {
        findings.extend(checks::injection::run(ctx, target).await);
    }
    if ctx.opts.wants("protocol") {
        findings.extend(checks::protocol::run(ctx, target).await);
    }
    if ctx.opts.wants("throttle") {
        findings.extend(checks::throttle::run(ctx, target).await);
    }
    findings
}
