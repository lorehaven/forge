//! Credential endpoints must slow guessing down. Under `--active`, a burst of wrong logins at every
//! login-shaped route must hit a 429 (or a lockout) before it can run unchecked.

use crate::finding::{Finding, Severity};
use crate::scan::Ctx;
use crate::wire::{Probe, Target};

const HINTS: [&str; 6] = [
    "login",
    "token",
    "password",
    "signin",
    "authenticate",
    "oauth",
];
const ATTEMPTS: usize = 40;
const BODY: &str = r#"{"username":"sectest-nonexistent","email":"sectest@example.invalid","password":"wrong-password-sectest","grant_type":"password"}"#;

pub async fn run(ctx: &Ctx, target: &Target) -> Vec<Finding> {
    if !ctx.opts.active {
        return Vec::new();
    }
    let mut findings = Vec::new();
    for route in ctx.routes_of(target).filter(|r| {
        r.method == "POST"
            && HINTS
                .iter()
                .any(|h| r.pattern.to_ascii_lowercase().contains(h))
    }) {
        let mut statuses = Vec::with_capacity(ATTEMPTS);
        for _ in 0..ATTEMPTS {
            let probe = Probe::new("POST", &route.sample_path()).json(BODY);
            if let Some(reply) = ctx.send_unthrottled(target, &probe).await {
                statuses.push(reply.status);
            }
        }
        if statuses.is_empty() || statuses.iter().all(|s| matches!(s, 404 | 405)) {
            continue;
        }
        if statuses.iter().any(|s| matches!(s, 200..=299)) {
            findings.push(Finding::new(
                Severity::Critical,
                "throttle",
                &target.name,
                "POST",
                &route.pattern,
                "a wrong-password login for a nonexistent user returned 2xx",
            ));
        }
        if !statuses.contains(&429) {
            findings.push(Finding::new(
                Severity::Medium,
                "throttle",
                &target.name,
                "POST",
                &route.pattern,
                format!("{ATTEMPTS} rapid wrong-credential attempts never hit a 429: password guessing is not throttled"),
            ));
        }
    }
    findings
}
