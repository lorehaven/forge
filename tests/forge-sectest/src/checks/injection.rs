//! Hostile input into places that parse it.
//!
//! Query strings go to every GET route and, under `--active`, malformed bodies to every write
//! route. The questions are always the same: did the handler crash, did it say too much, did it
//! echo markup back unescaped?

use super::leak_marker;
use crate::finding::{Finding, Severity};
use crate::scan::Ctx;
use crate::wire::{Probe, Target};

/// One query string that packs a different hostile value into every common parameter name.
const QUERY: &str = "?q=%27%22%3Cscript%3Esectest%3C%2Fscript%3E&limit=-1&page=99999999999999999999\
&id=%00&sort=%27%3B--&name=%24%7B7*7%7D&offset=NaN&filter=%5B%5D%5B%5D&search=%25%27+OR+1%3D1--";

const XSS: &str = "<script>sectest</script>";

fn bodies() -> Vec<(&'static str, &'static str, Vec<u8>)> {
    let deep = format!("{}{}", "[".repeat(3000), "]".repeat(3000)).into_bytes();
    vec![
        ("truncated JSON", "application/json", b"{".to_vec()),
        ("wrong top-level type", "application/json", b"[]".to_vec()),
        ("null body", "application/json", b"null".to_vec()),
        ("deeply nested JSON", "application/json", deep),
        ("integer overflow", "application/json", br#"{"id":99999999999999999999999,"limit":-99999999999}"#.to_vec()),
        ("invalid UTF-8", "application/json", vec![b'{', b'"', 0xff, 0xfe, b'"', b':', b'1', b'}']),
        ("XML external entity", "application/xml", br#"<?xml version="1.0"?><!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/passwd">]><x>&e;</x>"#.to_vec()),
        ("form with NUL", "application/x-www-form-urlencoded", b"a=%00&b=%ff%fe&c=".to_vec()),
    ]
}

pub async fn run(ctx: &Ctx, target: &Target) -> Vec<Finding> {
    let mut findings = Vec::new();

    for route in ctx.routes_of(target).filter(|r| r.method == "GET") {
        let path = format!("{}{QUERY}", route.sample_path());
        let Some(reply) = ctx
            .send(target, &ctx.authed(Probe::new("GET", &path)))
            .await
        else {
            continue;
        };
        if reply.status >= 500 {
            findings.push(Finding::new(
                Severity::Medium,
                "injection",
                &target.name,
                "GET",
                &route.pattern,
                format!(
                    "hostile query string crashed the handler ({})",
                    reply.status
                ),
            ));
        }
        if let Some((severity, what)) = leak_marker(&reply.body) {
            findings.push(Finding::new(
                severity,
                "injection",
                &target.name,
                "GET",
                &route.pattern,
                format!("response leaks {what}"),
            ));
        }
        if reply.is_html() && reply.body.contains(XSS) {
            findings.push(Finding::new(
                Severity::High,
                "injection",
                &target.name,
                "GET",
                &route.pattern,
                "REFLECTED XSS: markup from the query string is returned unescaped in an HTML page",
            ));
        }
    }

    if !ctx.opts.active {
        return findings;
    }
    for route in ctx
        .routes_of(target)
        .filter(|r| matches!(r.method.as_str(), "POST" | "PUT" | "PATCH"))
    {
        for (label, content_type, body) in bodies() {
            let mut probe = ctx.authed(Probe::new(&route.method, &route.sample_path()));
            probe = probe.header("Content-Type", content_type);
            probe.body = body;
            let Some(reply) = ctx.send(target, &probe).await else {
                continue;
            };
            if reply.status >= 500 {
                findings.push(Finding::new(
                    Severity::Medium,
                    "injection",
                    &target.name,
                    &route.method,
                    &route.pattern,
                    format!("{label} produced {} (unhandled input)", reply.status),
                ));
            }
            if let Some((severity, what)) = leak_marker(&reply.body) {
                findings.push(Finding::new(
                    severity,
                    "injection",
                    &target.name,
                    &route.method,
                    &route.pattern,
                    format!("{label}: response leaks {what}"),
                ));
            }
        }
    }
    findings
}
