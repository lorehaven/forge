//! How the app's requests become calls to the service, and how the answers become messages.

use gauge_app::{Msg, Request};
use gauge_core::{HostCard, HostDetail};
use quench_serde::json;

/// The service's path prefix from the page's own path: `/gauge/ui/home` is `/gauge`, `/ui/home` is no prefix.
#[must_use]
pub fn base_of(path: &str) -> String {
    let at = path
        .find("/ui/")
        .or_else(|| path.strip_suffix("/ui").map(str::len));
    at.map_or_else(String::new, |i| path[..i].trim_end_matches('/').to_string())
}

/// One path segment, percent-encoded.
#[must_use]
pub fn segment(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// What to ask of the service for a request; `None` for one the browser answers by itself.
#[must_use]
pub fn url_for(base: &str, request: &Request) -> Option<String> {
    match request {
        Request::Hosts => Some(format!("{base}/api/v1/hosts")),
        Request::Host(name) => Some(format!("{base}/api/v1/hosts/{}", segment(name))),
        Request::Language(_) => None,
    }
}

/// What the app is told of an answer.
#[must_use]
pub fn interpret(request: &Request, status: u16, body: &str) -> Msg {
    match status {
        200..=299 => match request {
            Request::Hosts => json::from_str::<Vec<HostCard>>(body).map(Msg::Hosts),
            Request::Host(_) => {
                json::from_str::<HostDetail>(body).map(|d| Msg::Detail(Box::new(d)))
            }
            Request::Language(_) => return Msg::Failed("nothing to ask".into()),
        }
        .unwrap_or_else(|e| Msg::Failed(format!("the service answered something unreadable: {e}"))),
        401 => Msg::SignedOut,
        403 => Msg::Failed("this account may not see Gauge: ask for access in gatehouse".into()),
        404 => match request {
            Request::Host(name) => Msg::NotFound(name.clone()),
            _ => Msg::Failed("the service answered 404".into()),
        },
        other => Msg::Failed(format!("the service answered {other}")),
    }
}

/// A call that did not get an answer.
#[must_use]
pub fn unreachable(why: &str) -> Msg {
    Msg::Failed(format!("cannot reach the service: {why}"))
}
