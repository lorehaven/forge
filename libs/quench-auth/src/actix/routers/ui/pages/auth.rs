//! Sending a browser to gatehouse.
//!
//! A relying party has no login page of its own: gatehouse owns the form, the
//! credentials and the session. All a service needs is the ability to hand the
//! browser over and to accept it back, which is what lives here.

use crate::actix::domain::realm;
use crate::actix::domain::sso_client::{self, SsoConfig};
use crate::prelude::JwtConfig;
use actix_web::HttpResponse;
use serde::{Deserialize, Serialize};
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

/// What a page's session watcher is told about the session it is holding.
#[derive(Serialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub username: Option<String>,
    pub roles: Vec<String>,
}

impl AuthStatus {
    pub fn anonymous() -> Self {
        Self {
            authenticated: false,
            username: None,
            roles: Vec::new(),
        }
    }
}

/// Answers "is the cookie I am holding still worth anything".
///
/// Always a `200`: this is a question about the session, not a request that
/// needs one, and answering `401` would make every watcher's error handling
/// carry the meaning instead of the body.
///
/// Reads the cookie and the signature only. Whether the session is still live
/// in the shared store is a heavier check that `is_ui_authenticated` does on
/// the requests that matter; here it would put a store round trip on every open
/// tab every minute, to catch a revocation moments earlier than the next real
/// request will anyway.
pub async fn auth_status(request: &actix_web::HttpRequest, config: &JwtConfig) -> HttpResponse {
    if !config.auth_enabled {
        return HttpResponse::Ok().json(AuthStatus {
            authenticated: true,
            username: Some("dev".to_string()),
            roles: vec!["admin".to_string()],
        });
    }

    let Some(cookie) = request.cookie(&realm::session_cookie_name()) else {
        return HttpResponse::Ok().json(AuthStatus::anonymous());
    };

    match config.decode_claims(cookie.value()).await {
        Ok(claims) => HttpResponse::Ok().json(AuthStatus {
            authenticated: true,
            username: Some(claims.sub),
            roles: claims
                .scope
                .split(',')
                .filter(|role| !role.is_empty())
                .map(str::to_string)
                .collect(),
        }),
        Err(_) => HttpResponse::Ok().json(AuthStatus::anonymous()),
    }
}

#[derive(Deserialize)]
struct RefreshedTokens {
    access_token: String,
    refresh_token: String,
}

/// Exchanges this browser's refresh cookie for a fresh token pair, so a tab
/// left open past the access token's expiry does not have to go through
/// gatehouse's login page to keep going.
///
/// Gatehouse is a distinct origin with no CORS policy open to relying
/// parties, and the refresh cookie is `SameSite=Lax`, so the browser cannot
/// call gatehouse directly from a fetch. This service makes that call on the
/// browser's behalf and hands back only its own new cookies - the same shape
/// `sso_client::callback` already uses to turn an authorization code into a
/// session.
pub async fn refresh_delegation(request: &actix_web::HttpRequest) -> HttpResponse {
    let Some(base) = realm::gatehouse_url() else {
        return HttpResponse::ServiceUnavailable().finish();
    };
    let Some(refresh_token) = request
        .cookie(&realm::refresh_cookie_name())
        .map(|cookie| cookie.value().to_string())
    else {
        return HttpResponse::Unauthorized().finish();
    };

    let tls_verify: bool = envmnt::get_or("GATEHOUSE_TLS_VERIFY", "true")
        .parse()
        .unwrap_or(true);
    let Ok(http) = reqwest::Client::builder()
        .danger_accept_invalid_certs(!tls_verify)
        .build()
    else {
        return HttpResponse::InternalServerError().finish();
    };

    let response = http
        .post(format!("{base}/api/v1/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await;
    let Ok(response) = response else {
        return HttpResponse::ServiceUnavailable().finish();
    };
    if !response.status().is_success() {
        return HttpResponse::Unauthorized().finish();
    }
    let Ok(tokens) = response.json::<RefreshedTokens>().await else {
        return HttpResponse::BadGateway().finish();
    };

    let mut refreshed = HttpResponse::Ok().json(AuthStatus {
        authenticated: true,
        username: None,
        roles: Vec::new(),
    });
    let _ = refreshed.add_cookie(&realm::session_cookie(tokens.access_token));
    let _ = refreshed.add_cookie(&realm::refresh_cookie(tokens.refresh_token));
    refreshed
}

/// Starts the authorization-code + PKCE round trip at gatehouse, so this
/// service ends up with a token it fetched itself rather than trusting a
/// realm-wide cookie gatehouse set directly. See `sso_client::authorize_redirect`.
pub fn login_delegation(request: &actix_web::HttpRequest, sso: &SsoConfig) -> HttpResponse {
    sso_client::authorize_redirect(request, sso)
}

/// `GET /ui/auth/callback` - completes the exchange `login_delegation` started.
pub async fn auth_callback(request: &actix_web::HttpRequest, sso: &SsoConfig) -> HttpResponse {
    sso_client::callback(request, sso).await
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
