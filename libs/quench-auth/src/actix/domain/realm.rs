//! Realm-wide auth settings.
//!
//! Every service in the estate shares one identity store (the `auth` schema),
//! one pair of cookies, and one login page (gatehouse). This module is the only
//! place those names are constructed - building them per service is what kept
//! the old setup from being single sign-on.

use actix_web::cookie::{Cookie, SameSite, time::Duration};

pub const DEFAULT_AUTH_SCHEMA: &str = "auth";
pub const DEFAULT_SESSION_COOKIE: &str = "forge_session";
pub const DEFAULT_REFRESH_COOKIE: &str = "forge_refresh";

/// Schema holding `users` and `sessions`. Shared by every service; only
/// gatehouse writes to it outside of session bookkeeping.
pub fn auth_schema() -> String {
    envmnt::get_or("AUTH_DB_SCHEMA", DEFAULT_AUTH_SCHEMA)
}

pub fn session_cookie_name() -> String {
    envmnt::get_or("AUTH_COOKIE_NAME", DEFAULT_SESSION_COOKIE)
}

pub fn refresh_cookie_name() -> String {
    envmnt::get_or("AUTH_REFRESH_COOKIE_NAME", DEFAULT_REFRESH_COOKIE)
}

/// Parent domain the cookies are scoped to, e.g. `.forge.example.com`. Unset
/// means host-only, which is what local development wants.
pub fn cookie_domain() -> Option<String> {
    non_empty(envmnt::get_or("AUTH_COOKIE_DOMAIN", ""))
}

/// Base URL of the gatehouse service. When unset, a service falls back to its
/// own login form - the pre-gatehouse behaviour.
pub fn gatehouse_url() -> Option<String> {
    non_empty(envmnt::get_or("GATEHOUSE_URL", "")).map(|url| url.trim_end_matches('/').to_string())
}

pub fn gatehouse_login_url(return_to: Option<&str>) -> Option<String> {
    gatehouse_endpoint("/ui/login", return_to)
}

pub fn gatehouse_logout_url(return_to: Option<&str>) -> Option<String> {
    gatehouse_endpoint("/ui/logout", return_to)
}

fn gatehouse_endpoint(path: &str, return_to: Option<&str>) -> Option<String> {
    let base = gatehouse_url()?;
    Some(match return_to {
        Some(target) if !target.is_empty() => {
            format!("{base}{path}?redirect={}", urlencoding::encode(target))
        }
        _ => format!("{base}{path}"),
    })
}

/// Session and refresh cookies, built identically everywhere.
///
/// `SameSite=Lax` rather than `Strict`: `Strict` suppresses the cookie on
/// cross-site top-level navigations, which is exactly the redirect back from
/// gatehouse after login.
pub fn session_cookie(token: impl Into<String>) -> Cookie<'static> {
    realm_cookie(session_cookie_name(), token.into())
}

pub fn refresh_cookie(token: impl Into<String>) -> Cookie<'static> {
    realm_cookie(refresh_cookie_name(), token.into())
}

/// Expired counterparts, for logout.
pub fn cleared_session_cookie() -> Cookie<'static> {
    cleared_cookie(session_cookie_name())
}

pub fn cleared_refresh_cookie() -> Cookie<'static> {
    cleared_cookie(refresh_cookie_name())
}

fn realm_cookie(name: String, value: String) -> Cookie<'static> {
    let mut builder = Cookie::build(name, value)
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(true);
    if let Some(domain) = cookie_domain() {
        builder = builder.domain(domain);
    }
    builder.finish()
}

fn cleared_cookie(name: String) -> Cookie<'static> {
    let mut builder = Cookie::build(name, "")
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(true)
        .max_age(Duration::seconds(0));
    if let Some(domain) = cookie_domain() {
        builder = builder.domain(domain);
    }
    builder.finish()
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
