//! The relying-party half of the authorization-code + PKCE flow: builds the
//! redirect to gatehouse's `/authorize`, and exchanges the code that comes
//! back at `/auth/callback` for this service's own token pair.
//!
//! Every relying party's `/ui/login` used to redirect straight to gatehouse's
//! login form and trust whatever came back on the shared realm cookie. This
//! module is what replaced that: gatehouse still owns the login form, but a
//! service ends up with a token it fetched itself, scoped to a client_id only
//! that service holds the secret for.

use crate::actix::domain::realm;
use actix_web::HttpResponse;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct SsoConfig {
    pub client_id: String,
    client_secret: String,
    tls_verify: bool,
}

impl SsoConfig {
    pub fn init() -> Self {
        let client_id = envmnt::get_or("GATEHOUSE_CLIENT_ID", "");
        let client_secret = envmnt::get_or("GATEHOUSE_CLIENT_SECRET", "");
        let tls_verify = envmnt::get_or("GATEHOUSE_TLS_VERIFY", "true")
            .parse()
            .unwrap_or(true);
        Self {
            client_id,
            client_secret,
            tls_verify,
        }
    }

    fn configured(&self) -> bool {
        !self.client_id.is_empty()
            && !self.client_secret.is_empty()
            && realm::gatehouse_url().is_some()
    }
}

#[derive(Serialize, Deserialize)]
struct AuthorizeState {
    state: String,
    code_verifier: String,
    redirect: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
}

/// Redirects the browser to gatehouse's `/authorize`, starting the code
/// exchange. `redirect` is where the browser lands after `/auth/callback`
/// completes - same validation as the old `?redirect=` handling
/// (`redirect_target`/`validated_redirect`).
pub fn authorize_redirect(request: &actix_web::HttpRequest, config: &SsoConfig) -> HttpResponse {
    let Some(base) = realm::gatehouse_url() else {
        return gatehouse_not_configured();
    };
    if !config.configured() {
        tracing::error!(
            "GATEHOUSE_CLIENT_ID / GATEHOUSE_CLIENT_SECRET not set: this service cannot start \
             the sign-in redirect"
        );
        return gatehouse_not_configured();
    }

    let destination = realm::ui_path("/home");
    let callback_url = absolute_url(request, &realm::ui_path("/auth/callback"));
    let state = random_urlsafe(24);
    let code_verifier = random_urlsafe(48);
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));

    let cookie_value = serde_json::to_string(&AuthorizeState {
        state: state.clone(),
        code_verifier,
        redirect: destination,
    })
    .unwrap_or_default();

    let url = format!(
        "{base}/api/v1/authorize?client_id={client_id}&redirect_uri={redirect_uri}&state={state}&code_challenge={code_challenge}&code_challenge_method=S256",
        client_id = urlencoding::encode(&config.client_id),
        redirect_uri = urlencoding::encode(&callback_url),
        state = urlencoding::encode(&state),
        code_challenge = urlencoding::encode(&code_challenge),
    );

    let mut response = HttpResponse::Found()
        .append_header(("Location", url))
        .finish();
    let _ = response.add_cookie(&realm::authorize_state_cookie(cookie_value));
    response
}

/// `GET /ui/auth/callback?code=...&state=...`: exchanges the code for this
/// service's own token pair and sets its local session cookies.
pub async fn callback(request: &actix_web::HttpRequest, config: &SsoConfig) -> HttpResponse {
    let Some(base) = realm::gatehouse_url() else {
        return gatehouse_not_configured();
    };

    let Some(cookie) = request.cookie(realm::AUTHORIZE_STATE_COOKIE) else {
        return callback_failed("missing authorize state");
    };
    let Ok(saved) = serde_json::from_str::<AuthorizeState>(cookie.value()) else {
        return callback_failed("corrupt authorize state");
    };

    let query = query_params(request);
    let (Some(code), Some(state)) = (query.get("code"), query.get("state")) else {
        return callback_failed("gatehouse did not return a code");
    };
    if state != &saved.state {
        return callback_failed("state mismatch");
    }

    let callback_url = absolute_url(request, &realm::ui_path("/auth/callback"));
    let http = match reqwest::Client::builder()
        .danger_accept_invalid_certs(!config.tls_verify)
        .build()
    {
        Ok(client) => client,
        Err(_) => return callback_failed("failed to build the token exchange client"),
    };

    let form = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", callback_url.as_str()),
        ("client_id", config.client_id.as_str()),
        ("client_secret", config.client_secret.as_str()),
        ("code_verifier", saved.code_verifier.as_str()),
    ];

    let response = http
        .post(format!("{base}/api/v1/token"))
        .form(&form)
        .send()
        .await;
    let Ok(response) = response else {
        return callback_failed("could not reach gatehouse");
    };
    if !response.status().is_success() {
        return callback_failed("gatehouse rejected the code exchange");
    }
    let Ok(tokens) = response.json::<TokenResponse>().await else {
        return callback_failed("gatehouse returned an unreadable token response");
    };

    let mut redirect_response = HttpResponse::Found()
        .append_header(("Location", saved.redirect))
        .finish();
    let _ = redirect_response.add_cookie(&realm::session_cookie(tokens.access_token));
    let _ = redirect_response.add_cookie(&realm::refresh_cookie(tokens.refresh_token));
    let _ = redirect_response.add_cookie(&realm::cleared_authorize_state_cookie());
    redirect_response
}

fn callback_failed(reason: &str) -> HttpResponse {
    tracing::warn!("sso callback failed: {reason}");
    let mut response = HttpResponse::Found()
        .append_header(("Location", realm::ui_path("/login")))
        .finish();
    let _ = response.add_cookie(&realm::cleared_authorize_state_cookie());
    response
}

fn gatehouse_not_configured() -> HttpResponse {
    tracing::error!(
        "GATEHOUSE_URL is not set: this service cannot sign anyone in, because gatehouse owns \
         the login form and the realm session"
    );
    HttpResponse::ServiceUnavailable().body("gatehouse is not configured")
}

fn absolute_url(request: &actix_web::HttpRequest, path: &str) -> String {
    let info = request.connection_info().clone();
    format!("{}://{}{}", info.scheme(), info.host(), path)
}

fn random_urlsafe(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

fn query_params(request: &actix_web::HttpRequest) -> std::collections::HashMap<String, String> {
    request
        .query_string()
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            let value = urlencoding::decode(value).ok()?.into_owned();
            Some((key.to_string(), value))
        })
        .collect()
}
