//! Who is asking: the address the reverse proxy saw.
//!
//! Gatehouse never sees the browser's socket - the ingress terminates it - so
//! the address comes from the proxy's headers. That is only as trustworthy as
//! the proxy: ingress-nginx (with `externalTrafficPolicy: Local`, so it sees the
//! real peer) overwrites `X-Real-IP` and `X-Forwarded-For` with it. Something
//! that can reach this service without going through the proxy can forge them,
//! which is why every per-IP limit here is backed by one that does not depend on
//! the address (per account, per email, and the daily mail budget).

use async_trait::async_trait;
use quench_http::prelude::{FromRequest, HttpError, Request};
use std::net::IpAddr;

/// A client address, or `unknown` when no usable one was sent. Unknown clients
/// share one bucket rather than each header value getting a bucket of its own -
/// otherwise a forged header would be an unlimited supply of fresh limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIp(pub String);

impl ClientIp {
    pub const UNKNOWN: &'static str = "unknown";

    /// `X-Real-IP`, else the last `X-Forwarded-For` entry (the one the nearest
    /// proxy appended; earlier entries are whatever the client claimed).
    pub fn from_headers(real_ip: Option<&str>, forwarded_for: Option<&str>) -> Self {
        let candidate = real_ip
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                forwarded_for
                    .and_then(|list| list.rsplit(',').next())
                    .map(str::trim)
            });
        match candidate.and_then(|value| value.parse::<IpAddr>().ok()) {
            Some(ip) => Self(ip.to_string()),
            None => Self(Self::UNKNOWN.to_string()),
        }
    }
}

#[async_trait]
impl FromRequest for ClientIp {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self::from_headers(
            req.header("x-real-ip"),
            req.header("x-forwarded-for"),
        ))
    }
}
