//! HTTP parsing attacks over raw sockets: request smuggling, header abuse, oversized heads.
//!
//! These hit whatever answers the port - the service itself, or a proxy in front of it - so a
//! failure can be the service's parser *or* the proxy/service disagreement that makes smuggling
//! possible. Each probe has one safe outcome: reject the message (4xx/5xx) or hang up.

use crate::finding::{Finding, Severity};
use crate::scan::Ctx;
use crate::wire::{self, RawReply, Target};
use std::fmt::Write;
use std::time::Duration;

/// The message was not served. A refusal, a hang-up, and silence (the parser still waiting for a
/// head that never terminates, as with bare-LF line endings) are all safe; only a success or
/// redirect status means the hostile message was acted on.
fn rejected(reply: &RawReply) -> bool {
    reply.first_status().is_none_or(|s| s >= 400)
}

struct Case {
    label: &'static str,
    wire: Vec<u8>,
    /// Severity when the server accepts the message instead of rejecting it.
    if_accepted: Severity,
    /// More than this many responses means a second request was smuggled in behind the first.
    max_responses: usize,
}

fn cases(host: &str) -> Vec<Case> {
    let head = |extra: &str| format!("POST / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n{extra}");
    let many = (0..1000).fold(String::new(), |mut out, i| {
        let _ = write!(out, "X-H{i}: v\r\n");
        out
    });
    let smuggled = format!("GET /sectest-smuggled HTTP/1.1\r\nHost: {host}\r\n\r\n");
    vec![
        Case {
            label: "Content-Length together with Transfer-Encoding (CL.TE desync)",
            wire: format!("{}\r\n0\r\n\r\n{smuggled}", head("Content-Length: 4\r\nTransfer-Encoding: chunked\r\n")).into_bytes(),
            if_accepted: Severity::Medium,
            max_responses: 1,
        },
        Case {
            label: "conflicting duplicate Content-Length headers",
            wire: format!("{}\r\nabc{smuggled}", head("Content-Length: 3\r\nContent-Length: 40\r\n")).into_bytes(),
            if_accepted: Severity::High,
            max_responses: 1,
        },
        Case {
            label: "obfuscated Transfer-Encoding (xchunked)",
            wire: format!("{}\r\n0\r\n\r\n{smuggled}", head("Transfer-Encoding: xchunked\r\nContent-Length: 6\r\n")).into_bytes(),
            if_accepted: Severity::High,
            max_responses: 1,
        },
        Case {
            label: "whitespace before the header colon",
            wire: format!("{}\r\n0\r\n\r\n{smuggled}", head("Transfer-Encoding : chunked\r\nContent-Length: 6\r\n")).into_bytes(),
            if_accepted: Severity::High,
            max_responses: 1,
        },
        Case {
            label: "bare LF line endings",
            wire: format!("GET / HTTP/1.1\nHost: {host}\nConnection: close\n\n").into_bytes(),
            if_accepted: Severity::Low,
            max_responses: 1,
        },
        Case {
            label: "NUL byte inside a header value",
            wire: format!("GET / HTTP/1.1\r\nHost: {host}\r\nX-Sectest: a\0b\r\nConnection: close\r\n\r\n").into_bytes(),
            if_accepted: Severity::Low,
            max_responses: 1,
        },
        Case {
            label: "obsolete header line folding",
            wire: format!("GET / HTTP/1.1\r\nHost: {host}\r\nX-Sectest: a\r\n folded\r\nConnection: close\r\n\r\n").into_bytes(),
            if_accepted: Severity::Low,
            max_responses: 1,
        },
        Case {
            label: "HTTP/1.1 request without a Host header",
            wire: b"GET / HTTP/1.1\r\nConnection: close\r\n\r\n".to_vec(),
            if_accepted: Severity::Low,
            max_responses: 1,
        },
        Case {
            label: "unsupported HTTP version",
            wire: format!("GET / HTTP/9.9\r\nHost: {host}\r\nConnection: close\r\n\r\n").into_bytes(),
            if_accepted: Severity::Low,
            max_responses: 1,
        },
        Case {
            label: "70 KiB header value",
            wire: format!("GET / HTTP/1.1\r\nHost: {host}\r\nX-Big: {}\r\nConnection: close\r\n\r\n", "a".repeat(70 * 1024)).into_bytes(),
            if_accepted: Severity::Medium,
            max_responses: 1,
        },
        Case {
            label: "20 KiB request target",
            wire: format!("GET /{} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n", "a".repeat(20 * 1024)).into_bytes(),
            if_accepted: Severity::Medium,
            max_responses: 1,
        },
        Case {
            label: "1000 headers",
            wire: format!("GET / HTTP/1.1\r\nHost: {host}\r\n{many}Connection: close\r\n\r\n").into_bytes(),
            if_accepted: Severity::Medium,
            max_responses: 1,
        },
    ]
}

pub async fn run(ctx: &Ctx, target: &Target) -> Vec<Finding> {
    let mut findings = Vec::new();
    let host = format!("{}:{}", target.host, target.port);

    for case in cases(&host) {
        let Ok(reply) = wire::raw(target, &case.wire, Duration::from_secs(6)).await else {
            continue;
        };
        if reply.responses() > case.max_responses {
            findings.push(Finding::new(
                Severity::High,
                "protocol",
                &target.name,
                "RAW",
                "/",
                format!(
                    "REQUEST SMUGGLING: {} - the server answered {} requests where one message was sent",
                    case.label,
                    reply.responses()
                ),
            ));
        } else if !rejected(&reply) {
            findings.push(Finding::new(
                case.if_accepted,
                "protocol",
                &target.name,
                "RAW",
                "/",
                format!(
                    "{} was accepted (status {:?}) instead of rejected",
                    case.label,
                    reply.first_status()
                ),
            ));
        }
    }

    // A malformed request must close the connection: bytes after it are never a new request.
    let after_error = format!(
        "GET / HTTP/1.1\r\nHost: {host}\r\nX-Sectest: a\0b\r\n\r\nGET /sectest-after-error HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    );
    if let Ok(reply) = wire::raw(target, after_error.as_bytes(), Duration::from_secs(6)).await
        && reply.responses() > 1
    {
        findings.push(Finding::new(
            Severity::High,
            "protocol",
            &target.name,
            "RAW",
            "/",
            "a request pipelined behind a malformed one was still executed",
        ));
    }

    if ctx.opts.slow {
        findings.extend(slowloris(target, &host).await);
    }
    findings
}

/// Holds a half-sent request open and measures how long the server tolerates it.
async fn slowloris(target: &Target, host: &str) -> Option<Finding> {
    let partial = format!("GET / HTTP/1.1\r\nHost: {host}\r\nX-Slow: ");
    let reply = wire::raw(target, partial.as_bytes(), Duration::from_secs(60)).await.ok()?;
    (!reply.closed).then(|| {
        Finding::new(
            Severity::Medium,
            "protocol",
            &target.name,
            "RAW",
            "/",
            "SLOWLORIS: an unfinished request head was held open for 60s without the server hanging up",
        )
    })
}
