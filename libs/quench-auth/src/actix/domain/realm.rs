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
/// Holds the PKCE verifier + `state` + return destination for the few seconds
/// between a relying party redirecting to `/authorize` and the browser coming
/// back to `/auth/callback`. Never leaves this service - gatehouse never sees it.
pub const AUTHORIZE_STATE_COOKIE: &str = "forge_authorize_state";

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

/// A short-lived, unnamed-elsewhere cookie for the authorize round trip.
/// Deliberately not domain-scoped even when `AUTH_COOKIE_DOMAIN` is set: it is
/// read back by the exact host that set it, never another relying party.
pub fn authorize_state_cookie(value: impl Into<String>) -> Cookie<'static> {
    Cookie::build(AUTHORIZE_STATE_COOKIE, value.into())
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(true)
        .max_age(Duration::minutes(5))
        .finish()
}

pub fn cleared_authorize_state_cookie() -> Cookie<'static> {
    Cookie::build(AUTHORIZE_STATE_COOKIE, "")
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(true)
        .max_age(Duration::seconds(0))
        .finish()
}

/// This service's own mount point (`BASE_PATH`), normalized to either `/` or
/// a leading-slash, no-trailing-slash prefix.
pub fn base_path() -> String {
    let raw = envmnt::get_or("BASE_PATH", "/");
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        let without_trailing = trimmed.trim_end_matches('/');
        if without_trailing.is_empty() {
            "/".to_string()
        } else if without_trailing.starts_with('/') {
            without_trailing.to_string()
        } else {
            format!("/{without_trailing}")
        }
    }
}

/// `path` under this service's own `/ui` scope, e.g. `ui_path("/home")`.
pub fn ui_path(path: &str) -> String {
    let base = base_path();
    if base == "/" {
        format!("/ui{path}")
    } else {
        format!("{base}/ui{path}")
    }
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
