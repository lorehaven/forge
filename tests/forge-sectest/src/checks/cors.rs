//! Cross-origin policy: a hostile origin must not be able to read authenticated responses.

use crate::finding::{Finding, Severity};
use crate::scan::Ctx;
use crate::wire::{Probe, Target};

const EVIL: &str = "https://sectest-evil.invalid";

pub async fn run(ctx: &Ctx, target: &Target) -> Vec<Finding> {
    let mut findings = Vec::new();
    let paths = ctx
        .routes_of(target)
        .filter(|r| r.method == "GET" && !r.has_params())
        .map(|r| r.pattern.clone())
        .take(10);

    for path in paths {
        for origin in [EVIL, "null"] {
            let probe = ctx.authed(Probe::new("GET", &path).header("Origin", origin));
            let Some(reply) = ctx.send(target, &probe).await else {
                continue;
            };
            let allow = reply.header("access-control-allow-origin").unwrap_or("");
            let credentials = reply
                .header("access-control-allow-credentials")
                .is_some_and(|v| v.eq_ignore_ascii_case("true"));
            let reflected = allow == origin;
            let severity = match (reflected || allow == "*", credentials) {
                (true, true) => Severity::High,
                (true, false) if reflected => Severity::Medium,
                _ => continue,
            };
            findings.push(Finding::new(
                severity,
                "cors",
                &target.name,
                "GET",
                &path,
                format!(
                    "Access-Control-Allow-Origin is {allow:?} for Origin {origin:?}{}",
                    if credentials {
                        " with credentials allowed: any site can read this as the user"
                    } else {
                        ""
                    }
                ),
            ));
        }

        let preflight = Probe::new("OPTIONS", &path)
            .header("Origin", EVIL)
            .header("Access-Control-Request-Method", "DELETE")
            .header("Access-Control-Request-Headers", "authorization");
        if let Some(reply) = ctx.send(target, &preflight).await {
            let allow = reply.header("access-control-allow-origin").unwrap_or("");
            let methods = reply.header("access-control-allow-methods").unwrap_or("");
            if (allow == EVIL || allow == "*") && methods.to_ascii_uppercase().contains("DELETE") {
                findings.push(Finding::new(
                    Severity::Medium,
                    "cors",
                    &target.name,
                    "OPTIONS",
                    &path,
                    format!("preflight from {EVIL} is approved for DELETE with Authorization"),
                ));
            }
        }
    }
    findings
}
