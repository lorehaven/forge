//! Sending a browser to gatehouse.
//!
//! A relying party has no login page of its own: gatehouse owns the form, the
//! credentials and the session. All a service needs is the ability to hand the
//! browser over and to accept it back, which is what lives here.

use crate::actix::domain::realm;
use actix_web::HttpResponse;
use serde::Deserialize;
use std::sync::LazyLock;

/// `?err=1` on the login page. Gatehouse renders the error; a relying party
/// only ever passes the parameter through.
#[derive(Deserialize)]
pub struct LoginQuery {
    pub err: Option<String>,
}

static BASE_PATH: LazyLock<String> = LazyLock::new(|| {
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
});

fn ui_path(path: &str) -> String {
    let base = BASE_PATH.as_str();
    if base == "/" {
        format!("/ui{path}")
    } else {
        format!("{base}/ui{path}")
    }
}

/// Hands the browser to gatehouse's login form, carrying a return address.
///
/// Gatehouse is required: a service with no `GATEHOUSE_URL` has no way for
/// anyone to sign in, so this reports that as a configuration error rather than
/// pretending to have a login page.
pub fn login_delegation(request: &actix_web::HttpRequest) -> HttpResponse {
    let return_to = absolute_url(request, &ui_path("/home"));
    match realm::gatehouse_login_url(Some(&return_to)) {
        Some(url) => redirect(url),
        None => gatehouse_not_configured(),
    }
}

/// Realm-wide logout, which is also gatehouse's to perform.
pub fn logout_delegation(request: &actix_web::HttpRequest) -> HttpResponse {
    let return_to = absolute_url(request, &ui_path("/login"));
    match realm::gatehouse_logout_url(Some(&return_to)) {
        Some(url) => redirect(url),
        None => gatehouse_not_configured(),
    }
}

fn redirect(location: String) -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", location))
        .finish()
}

fn gatehouse_not_configured() -> HttpResponse {
    tracing::error!(
        "GATEHOUSE_URL is not set: this service cannot sign anyone in, because \
         gatehouse owns the login form and the realm session"
    );
    HttpResponse::ServiceUnavailable().body("gatehouse is not configured")
}

/// Best-effort absolute URL for `path` on this service, so gatehouse can send
/// the browser back where it came from.
fn absolute_url(request: &actix_web::HttpRequest, path: &str) -> String {
    let info = request.connection_info().clone();
    format!("{}://{}{}", info.scheme(), info.host(), path)
}

/// `?redirect=` target, accepted only as a rooted same-origin path or a prefix
/// listed in `AUTH_REDIRECT_HOSTS` - an open redirect here would be a phishing
/// primitive.
pub fn redirect_target(request: &actix_web::HttpRequest) -> Option<String> {
    let query = request.query_string();
    let raw = query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "redirect").then_some(value)
    })?;
    let decoded = urlencoding::decode(raw).ok()?.into_owned();
    validated_redirect(&decoded)
}

pub fn validated_redirect(target: &str) -> Option<String> {
    // `//host` and `/\host` are protocol-relative: rooted to the eye, absolute
    // to a browser. Only a single-slash path counts as same-origin.
    let same_origin =
        target.starts_with('/') && !target.starts_with("//") && !target.starts_with("/\\");
    let allowed = same_origin
        || allowed_redirect_hosts()
            .iter()
            .any(|prefix| target.starts_with(prefix));
    allowed.then(|| target.to_string())
}

/// Prefixes a `?redirect=` may point at, from `AUTH_REDIRECT_HOSTS`
/// (comma-separated). Empty means same-origin paths only.
pub fn allowed_redirect_hosts() -> Vec<String> {
    envmnt::get_or("AUTH_REDIRECT_HOSTS", "")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('/').to_string())
        .collect()
}
