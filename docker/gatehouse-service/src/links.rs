//! The origin emailed links point at - configuration, never the request.
//!
//! A link that lands in someone's inbox carries a credential-equivalent token,
//! so its host must not be attacker-influenced: taking it from `Host` /
//! `X-Forwarded-Host` would let anyone request a reset for a victim with a
//! forged header and have the real mail server deliver a token to their site.

/// Where a browser reaches gatehouse: scheme, host and port, no path (the base
/// path is added by `ui_path`). No trailing slash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicBase(String);

/// What dev falls back to when neither variable is set (`foreman`'s port).
const DEV_DEFAULT: &str = "http://localhost:5443";

impl PublicBase {
    /// `PUBLIC_BASE_URL`, else the origin of `GATEHOUSE_URL` (which carries the
    /// base path, e.g. `https://host/gatehouse`), else the dev default.
    pub fn from_env() -> Self {
        let explicit = envmnt::get_or("PUBLIC_BASE_URL", "");
        let gatehouse = envmnt::get_or("GATEHOUSE_URL", "");
        Self::resolve(&explicit, &gatehouse)
    }

    pub fn resolve(explicit: &str, gatehouse_url: &str) -> Self {
        [explicit, gatehouse_url]
            .into_iter()
            .find_map(origin_of)
            .map_or_else(|| Self(DEV_DEFAULT.to_string()), Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `scheme://authority` of an http(s) URL; `None` for anything else, blank included.
fn origin_of(url: &str) -> Option<String> {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://")?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    Some(format!("{scheme}://{authority}"))
}
