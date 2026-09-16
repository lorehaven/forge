//! The one login form in the estate - form, credential check, and cookies
//! all live here; relying parties only ever redirect a browser to this page.

use crate::api::auth::issue_token_pair;
use crate::realm::{self as gh_realm, AuthOutcome};
use crate::ui::common::{UiPageKind, render_page, supported_locales, ui_path};
use async_trait::async_trait;
use http::StatusCode;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::realm;
use quench_auth::domain::session::SessionDb;
use quench_auth::domain::sso_client;
use quench_auth::http::domain::cookies::cookie_value;
use quench_auth::http::routers::ui::pages::auth::{
    LoginQuery, auth_status, redirect_target, refresh_delegation, validated_redirect,
};
use quench_db::prelude::Db;
use quench_http::prelude::{
    Form, FromRequest, HttpError, Inject, Query, Request, Response, get, post,
};
use quench_web::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    /// Where to send the browser afterwards; validated before use.
    #[serde(default)]
    pub redirect: Option<String>,
}

/// Notices beyond "wrong credentials" - separate from `LoginQuery`, which is
/// shared with every service's login redirect.
#[derive(Deserialize, Default)]
pub struct LoginNotices {
    #[serde(default)]
    pub registered: Option<String>,
    #[serde(default)]
    pub verified: Option<String>,
    #[serde(default)]
    pub reset: Option<String>,
    #[serde(default)]
    pub reset_requested: Option<String>,
    #[serde(default)]
    pub err: Option<String>,
}

/// Skips the credential form if a refresh cookie is still good enough to renew.
pub struct LoginContext {
    silent_refresh: Option<Response>,
    redirect: Option<String>,
}

#[async_trait]
impl FromRequest for LoginContext {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let redirect = redirect_target(req);
        let silent_refresh = match cookie_value(req, &realm::refresh_cookie_name()) {
            Some(refresh_token) => match sso_client::refresh(&refresh_token).await {
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
        Ok(Self {
            silent_refresh,
            redirect,
        })
    }
}

#[get("/ui/login")]
pub async fn login(
    ctx: LoginContext,
    Query(query): Query<LoginQuery>,
    Query(notices): Query<LoginNotices>,
) -> Response {
    match ctx.silent_refresh {
        Some(refreshed) => refreshed,
        None => render_login_page(ctx.redirect, query.err.as_deref() == Some("1"), &notices),
    }
}

#[get("/ui/login/")]
pub async fn login_slash(
    ctx: LoginContext,
    Query(query): Query<LoginQuery>,
    Query(notices): Query<LoginNotices>,
) -> Response {
    match ctx.silent_refresh {
        Some(refreshed) => refreshed,
        None => render_login_page(ctx.redirect, query.err.as_deref() == Some("1"), &notices),
    }
}

#[post("/ui/login")]
pub async fn login_submit(
    Form(form): Form<LoginForm>,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Inject(session_db): Inject<SessionDb>,
) -> Response {
    tracing::info!("login attempt for {}", form.username);

    let outcome = match gh_realm::authenticate(&db, &form.username, &form.password).await {
        Ok(outcome) => outcome,
        Err(err) => {
            tracing::error!("failed to authenticate {}: {:?}", form.username, err);
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
            tracing::warn!("login attempt for disabled account {}", form.username);
            return Response::new(StatusCode::FOUND)
                .header("Location", ui_path("/login?err=ui_login_account_disabled"));
        }
        AuthOutcome::Locked => {
            tracing::warn!("login attempt for locked account {}", form.username);
            return Response::new(StatusCode::FOUND)
                .header("Location", ui_path("/login?err=ui_login_account_locked"));
        }
        AuthOutcome::NotFound | AuthOutcome::WrongPassword => {
            tracing::warn!("invalid credentials for {}", form.username);
            return Response::new(StatusCode::FOUND).header("Location", ui_path("/login?err=1"));
        }
    };

    let Ok(tokens) = issue_token_pair(&config, &session_db, &user).await else {
        tracing::error!("failed to issue tokens for {}", user.username);
        return Response::new(StatusCode::FOUND).header("Location", ui_path("/login?err=1"));
    };

    let target = form
        .redirect
        .as_deref()
        .and_then(validated_redirect)
        .unwrap_or_else(|| ui_path("/home"));

    Response::new(StatusCode::FOUND)
        .header("Location", target)
        .append_header(
            "set-cookie",
            realm::session_cookie(tokens.access_token).to_string(),
        )
        .append_header(
            "set-cookie",
            realm::refresh_cookie(tokens.refresh_token).to_string(),
        )
}

#[derive(Deserialize)]
pub struct MfaQuery {
    pub pending: String,
    #[serde(default)]
    pub redirect: Option<String>,
    #[serde(default)]
    pub err: Option<String>,
}

#[get("/ui/login/mfa")]
pub async fn login_mfa(Query(query): Query<MfaQuery>) -> Response {
    render_mfa_page(
        &query.pending,
        query.redirect.as_deref(),
        query.err.as_deref() == Some("1"),
    )
}

#[derive(Deserialize)]
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
    Form(form): Form<MfaForm>,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Inject(session_db): Inject<SessionDb>,
) -> Response {
    let outcome = match gh_realm::authenticate_mfa(&db, &form.pending, &form.code).await {
        Ok(outcome) => outcome,
        Err(err) => {
            tracing::error!("failed to verify an MFA code: {:?}", err);
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
        AuthOutcome::MfaRequired { .. } | AuthOutcome::NotFound | AuthOutcome::WrongPassword => {
            return Response::new(StatusCode::FOUND).header(
                "Location",
                mfa_challenge_url(&form.pending, form.redirect.as_deref(), true),
            );
        }
    };

    let Ok(tokens) = issue_token_pair(&config, &session_db, &user).await else {
        tracing::error!("failed to issue tokens for {}", user.username);
        return Response::new(StatusCode::FOUND).header("Location", ui_path("/login?err=1"));
    };

    let target = form
        .redirect
        .as_deref()
        .and_then(validated_redirect)
        .unwrap_or_else(|| ui_path("/home"));

    Response::new(StatusCode::FOUND)
        .header("Location", target)
        .append_header(
            "set-cookie",
            realm::session_cookie(tokens.access_token).to_string(),
        )
        .append_header(
            "set-cookie",
            realm::refresh_cookie(tokens.refresh_token).to_string(),
        )
}

pub fn mfa_challenge_url(pending: &str, redirect: Option<&str>, err: bool) -> String {
    let mut url = format!(
        "{}?pending={}",
        ui_path("/login/mfa"),
        urlencoding::encode(pending)
    );
    if let Some(target) = redirect.filter(|value| !value.is_empty()) {
        url.push_str(&format!("&redirect={}", urlencoding::encode(target)));
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
        Ok(Self(refresh_delegation(req).await))
    }
}

#[post("/ui/refresh")]
pub async fn refresh(RefreshResponse(resp): RefreshResponse) -> Response {
    resp
}

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
        tracing::debug!("logout revoke result: {revoked:?}");
    }

    let target = ctx.redirect.unwrap_or_else(|| ui_path("/login"));

    Response::new(StatusCode::FOUND)
        .header("Location", target)
        .append_header("set-cookie", realm::cleared_session_cookie().to_string())
        .append_header("set-cookie", realm::cleared_refresh_cookie().to_string())
}

pub fn render_login_page(
    redirect: Option<String>,
    error: bool,
    notices: &LoginNotices,
) -> Response {
    // Carried through so a login that started at sage returns to sage.
    let mut login_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/login"))
        .child(
            label()
                .attr("for", "username")
                .attr("data-i18n", "ui_login_username"),
        )
        .child(
            element("input")
                .attr("type", "text")
                .attr("id", "username")
                .attr("name", "username")
                .attr("autocomplete", "username")
                .attr("autofocus", "autofocus")
                .attr("required", "required"),
        )
        .child(
            label()
                .attr("for", "password")
                .attr("data-i18n", "ui_login_password"),
        )
        .child(
            element("input")
                .attr("type", "password")
                .attr("id", "password")
                .attr("name", "password")
                .attr("autocomplete", "current-password")
                .attr("required", "required"),
        )
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_login_submit"),
        );

    if let Some(target) = redirect.filter(|value| !value.is_empty()) {
        login_form = login_form.child(
            element("input")
                .attr("type", "hidden")
                .attr("name", "redirect")
                .attr("value", target),
        );
    }

    if error {
        login_form = login_form.child(
            p().class("error")
                .attr("data-i18n", "ui_login_invalid_credentials"),
        );
    } else if let Some(key) = login_error_key(notices) {
        login_form = login_form.child(p().class("error").attr("data-i18n", key));
    } else if let Some(key) = login_ok_key(notices) {
        login_form = login_form.child(p().class("admin-notice ok").attr("data-i18n", key));
    }

    login_form = login_form
        .child(
            a().class("admin-hint")
                .attr("href", ui_path("/forgot-password"))
                .attr("data-i18n", "ui_login_forgot_password"),
        )
        .child(
            a().class("admin-hint")
                .attr("href", ui_path("/register"))
                .attr("data-i18n", "ui_login_register"),
        );

    // No page shell top panel here, so the card carries brand + language switch.
    let login_bar = div()
        .class("login-bar")
        .child(
            span()
                .class("login-brand")
                .attr("data-i18n", "header_label"),
        )
        .child(locale_switch(Some(supported_locales()), None));

    let credentials = div()
        .class("login-credentials")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_login_sign_in"),
        )
        .child(div().class("meta-list").child(login_form));

    render_page(
        StatusCode::OK,
        content().class("container-fluid login-layout").child(
            div()
                .class("panel login-panel")
                .child(login_bar)
                .child(credentials),
        ),
        UiPageKind::Auth,
    )
}

/// Checked against a fixed list, not trusted from the query string.
pub fn login_error_key(notices: &LoginNotices) -> Option<&'static str> {
    match notices.err.as_deref() {
        Some("ui_login_verify_invalid") => Some("ui_login_verify_invalid"),
        Some("ui_login_reset_invalid") => Some("ui_login_reset_invalid"),
        Some("ui_login_account_disabled") => Some("ui_login_account_disabled"),
        Some("ui_login_account_locked") => Some("ui_login_account_locked"),
        _ => None,
    }
}

/// Order matters: a successful reset is more specific than "link sent".
pub fn login_ok_key(notices: &LoginNotices) -> Option<&'static str> {
    if notices.reset.is_some() {
        Some("ui_login_reset_ok")
    } else if notices.reset_requested.is_some() {
        Some("ui_login_reset_requested_ok")
    } else if notices.verified.is_some() {
        Some("ui_login_verified_ok")
    } else if notices.registered.is_some() {
        Some("ui_login_registered_ok")
    } else {
        None
    }
}

pub fn render_mfa_page(pending: &str, redirect: Option<&str>, error: bool) -> Response {
    let mut mfa_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/login/mfa"))
        .child(
            element("input")
                .attr("type", "hidden")
                .attr("name", "pending")
                .attr("value", pending),
        )
        .child(
            label()
                .attr("for", "code")
                .attr("data-i18n", "ui_login_mfa_code"),
        )
        .child(
            element("input")
                .attr("type", "text")
                .attr("id", "code")
                .attr("name", "code")
                .attr("inputmode", "numeric")
                .attr("autocomplete", "one-time-code")
                .attr("autofocus", "autofocus")
                .attr("required", "required"),
        )
        .child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_login_mfa_hint"),
        )
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_login_mfa_submit"),
        );

    if let Some(target) = redirect.filter(|value| !value.is_empty()) {
        mfa_form = mfa_form.child(
            element("input")
                .attr("type", "hidden")
                .attr("name", "redirect")
                .attr("value", target),
        );
    }

    if error {
        mfa_form = mfa_form.child(p().class("error").attr("data-i18n", "ui_login_mfa_invalid"));
    }

    render_auth_page("ui_login_mfa_title", mfa_form)
}

fn render_auth_page(title_key: &'static str, inner_form: Element) -> Response {
    let bar = div()
        .class("login-bar")
        .child(
            span()
                .class("login-brand")
                .attr("data-i18n", "header_label"),
        )
        .child(locale_switch(Some(supported_locales()), None));

    let credentials = div()
        .class("login-credentials")
        .child(div().class("panel-title").attr("data-i18n", title_key))
        .child(div().class("meta-list").child(inner_form));

    render_page(
        StatusCode::OK,
        content().class("container-fluid login-layout").child(
            div()
                .class("panel login-panel")
                .child(bar)
                .child(credentials),
        ),
        UiPageKind::Auth,
    )
}

/// Anything under `/ui` that is not a page sends you to the login form.
pub fn login_redirect() -> Response {
    Response::new(StatusCode::FOUND).header("Location", ui_path("/login"))
}

pub fn register_routes() {
    let _ = login as fn(_, _, _) -> _;
    let _ = login_slash as fn(_, _, _) -> _;
    let _ = login_submit as fn(_, _, _, _) -> _;
    let _ = login_mfa as fn(_) -> _;
    let _ = login_mfa_submit as fn(_, _, _, _) -> _;
    let _ = status as fn(_) -> _;
    let _ = refresh as fn(_) -> _;
    let _ = logout as fn(_, _) -> _;
}
