//! The one login in the estate.
//!
//! Credential check and cookies live here; relying parties only ever redirect a browser to `/ui/login`. The form is drawn by the client; it posts here and the answer is a redirect with the cookies set.

use crate::api::auth::{issue_token_pair, refresh_session};
use crate::realm::{self as gh_realm, AuthOutcome};
use crate::ui::common::ui_path;
use crate::ui::locale::{LocaleCookie, default_locale_cookie};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::realm;
use quench_auth::domain::session::SessionDb;
use quench_auth::http::domain::cookies::cookie_value;
use quench_auth::http::routers::ui::pages::auth::{
    auth_status, redirect_target, validated_redirect,
};
use quench_core::async_trait;
use quench_db::prelude::Db;
use quench_http::http::StatusCode;
use quench_http::prelude::{
    Form, FromRequest, HttpError, Inject, Query, Request, Response, get, post,
};
use quench_serde::Deserialize;
use std::fmt::Write as _;

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    /// Where to send the browser afterwards; validated before use.
    #[serde(default)]
    pub redirect: Option<String>,
}

/// Gatehouse renews sessions itself - see `refresh_session` for why not over HTTP.
async fn refresh_from_container(
    req: &Request,
    refresh_token: &str,
) -> Option<crate::api::auth::TokenResponse> {
    let config = req.container().get::<JwtConfig>().ok()?;
    let db = req.container().get::<Db>().ok()?;
    let sessions = req.container().get::<SessionDb>().ok()?;
    refresh_session(&config, &db, &sessions, refresh_token).await
}

/// Skips the credential form if a refresh cookie is still good enough to renew.
pub struct LoginContext {
    silent_refresh: Option<Response>,
}

#[async_trait]
impl FromRequest for LoginContext {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let redirect = redirect_target(req);
        let silent_refresh = match cookie_value(req, &realm::refresh_cookie_name()) {
            Some(refresh_token) => match refresh_from_container(req, &refresh_token).await {
                Some(tokens) => {
                    let target = redirect.clone().unwrap_or_else(|| ui_path("/home"));
                    Some(
                        Response::new(StatusCode::FOUND)
                            .header("Location", target)
                            .append_header(
                                "set-cookie",
                                realm::session_cookie(tokens.access_token).to_string(),
                            )
                            .append_header(
                                "set-cookie",
                                realm::refresh_cookie(tokens.refresh_token).to_string(),
                            ),
                    )
                }
                None => None,
            },
            None => None,
        };
        Ok(Self { silent_refresh })
    }
}

/// The sign-in page.
///
/// The client draws the form (the notices a redirect carries come in its query). A refresh cookie that
/// is still good renews the session here instead, and the person goes where they were going.
#[get("/ui/login")]
pub async fn login(ctx: LoginContext) -> Response {
    ctx.silent_refresh.unwrap_or_else(super::client::public)
}

#[get("/ui/login/")]
pub async fn login_slash(ctx: LoginContext) -> Response {
    ctx.silent_refresh.unwrap_or_else(super::client::public)
}

#[post("/ui/login")]
pub async fn login_submit(
    locale_cookie: LocaleCookie,
    Form(form): Form<LoginForm>,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Inject(session_db): Inject<SessionDb>,
) -> Response {
    quench_log::info!("login attempt for {}", form.username);

    let outcome = match gh_realm::authenticate(&db, &form.username, &form.password).await {
        Ok(outcome) => outcome,
        Err(err) => {
            quench_log::error!("failed to authenticate {}: {:?}", form.username, err);
            return Response::new(StatusCode::FOUND).header("Location", ui_path("/login?err=1"));
        }
    };

    let user = match outcome {
        AuthOutcome::Success(user) => user,
        AuthOutcome::MfaRequired { pending } => {
            return Response::new(StatusCode::FOUND).header(
                "Location",
                mfa_challenge_url(&pending, form.redirect.as_deref(), false),
            );
        }
        AuthOutcome::Disabled => {
            quench_log::warn!("login attempt for disabled account {}", form.username);
            return Response::new(StatusCode::FOUND)
                .header("Location", ui_path("/login?err=ui_login_account_disabled"));
        }
        AuthOutcome::Locked => {
            quench_log::warn!("login attempt for locked account {}", form.username);
            return Response::new(StatusCode::FOUND)
                .header("Location", ui_path("/login?err=ui_login_account_locked"));
        }
        AuthOutcome::EmailUnverified => {
            quench_log::warn!("login attempt for unverified account {}", form.username);
            return Response::new(StatusCode::FOUND)
                .header("Location", ui_path("/login?err=ui_login_email_unverified"));
        }
        AuthOutcome::NotFound | AuthOutcome::WrongPassword => {
            quench_log::warn!("invalid credentials for {}", form.username);
            return Response::new(StatusCode::FOUND).header("Location", ui_path("/login?err=1"));
        }
    };

    let Ok(tokens) = issue_token_pair(&config, &session_db, &user).await else {
        quench_log::error!("failed to issue tokens for {}", user.username);
        return Response::new(StatusCode::FOUND).header("Location", ui_path("/login?err=1"));
    };

    let target = form
        .redirect
        .as_deref()
        .and_then(validated_redirect)
        .unwrap_or_else(|| ui_path("/home"));

    let response = Response::new(StatusCode::FOUND)
        .header("Location", target)
        .append_header(
            "set-cookie",
            realm::session_cookie(tokens.access_token).to_string(),
        )
        .append_header(
            "set-cookie",
            realm::refresh_cookie(tokens.refresh_token).to_string(),
        );
    match default_locale_cookie(&locale_cookie, user.preferred_locale.as_deref()) {
        Some(cookie) => response.append_header("set-cookie", cookie),
        None => response,
    }
}

#[derive(Debug, Deserialize)]
pub struct MfaQuery {
    pub pending: String,
    #[serde(default)]
    pub redirect: Option<String>,
    #[serde(default)]
    pub err: Option<String>,
}

/// The code-entry page; `pending` (which the client puts in the form) must be there.
#[get("/ui/login/mfa")]
pub async fn login_mfa(Query(_query): Query<MfaQuery>) -> Response {
    super::client::public()
}

#[derive(Debug, Deserialize)]
pub struct MfaForm {
    pub pending: String,
    pub code: String,
    #[serde(default)]
    pub redirect: Option<String>,
}

/// Code-entry step after `login_submit` found MFA required; `pending` proves
/// the password step already happened.
#[post("/ui/login/mfa")]
pub async fn login_mfa_submit(
    locale_cookie: LocaleCookie,
    Form(form): Form<MfaForm>,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Inject(session_db): Inject<SessionDb>,
) -> Response {
    let outcome = match gh_realm::authenticate_mfa(&db, &form.pending, &form.code).await {
        Ok(outcome) => outcome,
        Err(err) => {
            quench_log::error!("failed to verify an MFA code: {:?}", err);
            return Response::new(StatusCode::FOUND).header("Location", ui_path("/login?err=1"));
        }
    };

    let user = match outcome {
        AuthOutcome::Success(user) => user,
        AuthOutcome::Disabled => {
            return Response::new(StatusCode::FOUND)
                .header("Location", ui_path("/login?err=ui_login_account_disabled"));
        }
        AuthOutcome::Locked => {
            return Response::new(StatusCode::FOUND)
                .header("Location", ui_path("/login?err=ui_login_account_locked"));
        }
        // Stale token and wrong code look identical - an attacker can't tell them apart.
        AuthOutcome::MfaRequired { .. }
        | AuthOutcome::EmailUnverified
        | AuthOutcome::NotFound
        | AuthOutcome::WrongPassword => {
            return Response::new(StatusCode::FOUND).header(
                "Location",
                mfa_challenge_url(&form.pending, form.redirect.as_deref(), true),
            );
        }
    };

    let Ok(tokens) = issue_token_pair(&config, &session_db, &user).await else {
        quench_log::error!("failed to issue tokens for {}", user.username);
        return Response::new(StatusCode::FOUND).header("Location", ui_path("/login?err=1"));
    };

    let target = form
        .redirect
        .as_deref()
        .and_then(validated_redirect)
        .unwrap_or_else(|| ui_path("/home"));

    let response = Response::new(StatusCode::FOUND)
        .header("Location", target)
        .append_header(
            "set-cookie",
            realm::session_cookie(tokens.access_token).to_string(),
        )
        .append_header(
            "set-cookie",
            realm::refresh_cookie(tokens.refresh_token).to_string(),
        );
    match default_locale_cookie(&locale_cookie, user.preferred_locale.as_deref()) {
        Some(cookie) => response.append_header("set-cookie", cookie),
        None => response,
    }
}

#[must_use]
pub fn mfa_challenge_url(pending: &str, redirect: Option<&str>, err: bool) -> String {
    let mut url = format!(
        "{}?pending={}",
        ui_path("/login/mfa"),
        quench_serde::urlencoded::percent_encode(pending)
    );
    if let Some(target) = redirect.filter(|value| !value.is_empty()) {
        let _ = write!(
            url,
            "&redirect={}",
            quench_serde::urlencoded::percent_encode(target)
        );
    }
    if err {
        url.push_str("&err=1");
    }
    url
}

pub struct AuthStatusResponse(Response);

#[async_trait]
impl FromRequest for AuthStatusResponse {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let config = req
            .container()
            .get::<JwtConfig>()
            .map_err(|e| HttpError::status(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(Self(auth_status(req, &config).await))
    }
}

/// What the page shell's session watcher polls - never redirects from `/login`.
#[get("/ui/status")]
pub async fn status(AuthStatusResponse(resp): AuthStatusResponse) -> Response {
    resp
}

pub struct RefreshResponse(Response);

#[async_trait]
impl FromRequest for RefreshResponse {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Some(refresh_token) = cookie_value(req, &realm::refresh_cookie_name()) else {
            return Ok(Self(Response::new(StatusCode::UNAUTHORIZED)));
        };
        let Some(tokens) = refresh_from_container(req, &refresh_token).await else {
            return Ok(Self(Response::new(StatusCode::UNAUTHORIZED)));
        };
        let body = Response::json(
            StatusCode::OK,
            &quench_serde::json!({ "authenticated": true }),
        )
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
        Ok(Self(
            body.append_header(
                "set-cookie",
                realm::session_cookie(tokens.access_token).to_string(),
            )
            .append_header(
                "set-cookie",
                realm::refresh_cookie(tokens.refresh_token).to_string(),
            ),
        ))
    }
}

#[post("/ui/refresh")]
pub async fn refresh(RefreshResponse(resp): RefreshResponse) -> Response {
    resp
}

#[derive(Debug)]
pub struct LogoutContext {
    refresh_cookie: Option<String>,
    redirect: Option<String>,
}

#[async_trait]
impl FromRequest for LogoutContext {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self {
            refresh_cookie: cookie_value(req, &realm::refresh_cookie_name()),
            redirect: redirect_target(req),
        })
    }
}

/// Realm-wide logout: revokes the session, clears the shared cookie.
#[get("/ui/logout")]
pub async fn logout(ctx: LogoutContext, Inject(session_db): Inject<SessionDb>) -> Response {
    if let Some(refresh_token) = ctx.refresh_cookie {
        let revoked = session_db.revoke_by_refresh_token(&refresh_token).await;
        quench_log::debug!("logout revoke result: {revoked:?}");
    }

    let target = ctx.redirect.unwrap_or_else(|| ui_path("/login"));

    Response::new(StatusCode::FOUND)
        .header("Location", target)
        .append_header("set-cookie", realm::cleared_session_cookie().to_string())
        .append_header("set-cookie", realm::cleared_refresh_cookie().to_string())
}

/// Anything under `/ui` that is not a page sends you to the login form.
#[must_use]
pub fn login_redirect() -> Response {
    Response::new(StatusCode::FOUND).header("Location", ui_path("/login"))
}

pub fn register_routes() {
    let _ = login as fn(_) -> _;
    let _ = login_slash as fn(_) -> _;
    let _ = login_submit as fn(_, _, _, _, _) -> _;
    let _ = login_mfa as fn(_) -> _;
    let _ = login_mfa_submit as fn(_, _, _, _, _) -> _;
    let _ = status as fn(_) -> _;
    let _ = refresh as fn(_) -> _;
    let _ = logout as fn(_, _) -> _;
}

impl std::fmt::Debug for LoginContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("LoginContext { .. }")
    }
}

impl std::fmt::Debug for AuthStatusResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AuthStatusResponse { .. }")
    }
}

impl std::fmt::Debug for RefreshResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RefreshResponse { .. }")
    }
}
