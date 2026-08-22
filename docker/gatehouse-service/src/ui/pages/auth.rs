//! The one login form in the estate.
//!
//! The form, the credential check and the cookies live here rather than in
//! `quench-auth`: relying parties only ever redirect a browser to this page.

use crate::api::auth::issue_token_pair;
use crate::realm::{self as gh_realm, AuthOutcome};
use crate::ui::common::{UiPageKind, render_page, supported_locales, ui_path};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_auth::actix::routers::ui::pages::auth::{
    LoginQuery, redirect_target, refresh_delegation, validated_redirect,
};
use quench_auth::prelude::realm;
use quench_auth::prelude::{JwtConfig, SessionDb};
use quench_db::prelude::Db;
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    /// Where to send the browser afterwards. Carried through the form so it
    /// survives the POST; validated before use.
    #[serde(default)]
    pub redirect: Option<String>,
}

/// Notices this page can show beyond "wrong credentials" - one per thing that
/// can redirect here from registration or password reset. A second, separate
/// `web::Query` extractor rather than folding these into `LoginQuery`: that
/// type is shared with every other service's login redirect and has no
/// business knowing gatehouse grew a registration flow.
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

#[get("/login")]
pub async fn login(
    request: HttpRequest,
    query: web::Query<LoginQuery>,
    notices: web::Query<LoginNotices>,
) -> impl Responder {
    match try_silent_refresh(&request).await {
        Some(refreshed) => refreshed,
        None => render_login_page(&request, query.err.as_deref() == Some("1"), &notices),
    }
}

#[get("/login/")]
pub async fn login_slash(
    request: HttpRequest,
    query: web::Query<LoginQuery>,
    notices: web::Query<LoginNotices>,
) -> impl Responder {
    match try_silent_refresh(&request).await {
        Some(refreshed) => refreshed,
        None => render_login_page(&request, query.err.as_deref() == Some("1"), &notices),
    }
}

/// Mirrors `quench_auth`'s `login_delegation`: skip the credential form if a
/// `forge_refresh` cookie is still good enough to renew.
async fn try_silent_refresh(request: &HttpRequest) -> Option<HttpResponse> {
    let refresh_token = request
        .cookie(&realm::refresh_cookie_name())
        .map(|cookie| cookie.value().to_string())?;
    let tokens = quench_auth::actix::domain::sso_client::refresh(&refresh_token).await?;
    let target = redirect_target(request).unwrap_or_else(|| ui_path("/home"));
    Some(
        HttpResponse::Found()
            .cookie(realm::session_cookie(tokens.access_token))
            .cookie(realm::refresh_cookie(tokens.refresh_token))
            .append_header(("Location", target))
            .finish(),
    )
}

#[post("/login")]
pub async fn login_submit(
    form: web::Form<LoginForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    session_db: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    tracing::info!("login attempt for {}", form.username);

    let outcome = match gh_realm::authenticate(&db, &form.username, &form.password).await {
        Ok(outcome) => outcome,
        Err(err) => {
            tracing::error!("failed to authenticate {}: {:?}", form.username, err);
            return HttpResponse::Found()
                .append_header(("Location", ui_path("/login?err=1")))
                .finish();
        }
    };

    let user = match outcome {
        AuthOutcome::Success(user) => user,
        AuthOutcome::MfaRequired { pending } => {
            return HttpResponse::Found()
                .append_header((
                    "Location",
                    mfa_challenge_url(&pending, form.redirect.as_deref(), false),
                ))
                .finish();
        }
        AuthOutcome::Disabled => {
            tracing::warn!("login attempt for disabled account {}", form.username);
            return HttpResponse::Found()
                .append_header(("Location", ui_path("/login?err=ui_login_account_disabled")))
                .finish();
        }
        AuthOutcome::Locked => {
            tracing::warn!("login attempt for locked account {}", form.username);
            return HttpResponse::Found()
                .append_header(("Location", ui_path("/login?err=ui_login_account_locked")))
                .finish();
        }
        AuthOutcome::NotFound | AuthOutcome::WrongPassword => {
            tracing::warn!("invalid credentials for {}", form.username);
            return HttpResponse::Found()
                .append_header(("Location", ui_path("/login?err=1")))
                .finish();
        }
    };

    let Ok(tokens) = issue_token_pair(&config, &session_db, &user).await else {
        tracing::error!("failed to issue tokens for {}", user.username);
        return HttpResponse::Found()
            .append_header(("Location", ui_path("/login?err=1")))
            .finish();
    };

    let target = form
        .redirect
        .as_deref()
        .and_then(validated_redirect)
        .unwrap_or_else(|| ui_path("/home"));

    HttpResponse::Found()
        .cookie(realm::session_cookie(tokens.access_token))
        .cookie(realm::refresh_cookie(tokens.refresh_token))
        .append_header(("Location", target))
        .finish()
}

#[derive(Deserialize)]
pub struct MfaQuery {
    pub pending: String,
    #[serde(default)]
    pub redirect: Option<String>,
    #[serde(default)]
    pub err: Option<String>,
}

#[get("/login/mfa")]
pub async fn login_mfa(query: web::Query<MfaQuery>) -> impl Responder {
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

/// The code-entry step of a login that `login_submit` found to require MFA.
/// `pending` proves the password step already happened - see
/// [`crate::realm::authenticate_mfa`].
#[post("/login/mfa")]
pub async fn login_mfa_submit(
    form: web::Form<MfaForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    session_db: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    let outcome = match gh_realm::authenticate_mfa(&db, &form.pending, &form.code).await {
        Ok(outcome) => outcome,
        Err(err) => {
            tracing::error!("failed to verify an MFA code: {:?}", err);
            return HttpResponse::Found()
                .append_header(("Location", ui_path("/login?err=1")))
                .finish();
        }
    };

    let user = match outcome {
        AuthOutcome::Success(user) => user,
        AuthOutcome::Disabled => {
            return HttpResponse::Found()
                .append_header(("Location", ui_path("/login?err=ui_login_account_disabled")))
                .finish();
        }
        AuthOutcome::Locked => {
            return HttpResponse::Found()
                .append_header(("Location", ui_path("/login?err=ui_login_account_locked")))
                .finish();
        }
        // `authenticate_mfa` never returns `MfaRequired` itself - it is the
        // second step - and reports a stale/tampered pending token or a
        // since-vanished account the same way as a wrong code, so an
        // attacker cannot tell them apart.
        AuthOutcome::MfaRequired { .. } | AuthOutcome::NotFound | AuthOutcome::WrongPassword => {
            return HttpResponse::Found()
                .append_header((
                    "Location",
                    mfa_challenge_url(&form.pending, form.redirect.as_deref(), true),
                ))
                .finish();
        }
    };

    let Ok(tokens) = issue_token_pair(&config, &session_db, &user).await else {
        tracing::error!("failed to issue tokens for {}", user.username);
        return HttpResponse::Found()
            .append_header(("Location", ui_path("/login?err=1")))
            .finish();
    };

    let target = form
        .redirect
        .as_deref()
        .and_then(validated_redirect)
        .unwrap_or_else(|| ui_path("/home"));

    HttpResponse::Found()
        .cookie(realm::session_cookie(tokens.access_token))
        .cookie(realm::refresh_cookie(tokens.refresh_token))
        .append_header(("Location", target))
        .finish()
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

/// What the page shell's session watcher polls.
///
/// Gatehouse needs this as much as a relying party does: `/ui/home` is the
/// estate's launcher, which is exactly the kind of page somebody leaves open.
/// The watcher does not turn the login page into a redirect loop, because it
/// refuses to redirect from a `/login` path in the first place.
#[get("/status")]
pub async fn status(request: HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    quench_auth::actix::routers::ui::pages::auth::auth_status(&request, &config).await
}

#[post("/refresh")]
pub async fn refresh(request: HttpRequest) -> impl Responder {
    refresh_delegation(&request).await
}

/// Realm-wide logout: revokes the session and clears the shared cookie, so
/// every service sees the user as signed out.
#[get("/logout")]
pub async fn logout(request: HttpRequest, session_db: web::Data<Arc<SessionDb>>) -> impl Responder {
    if let Some(cookie) = request.cookie(&realm::refresh_cookie_name()) {
        let revoked = session_db.revoke_by_refresh_token(cookie.value()).await;
        tracing::debug!("logout revoke result: {revoked:?}");
    }

    let target = redirect_target(&request).unwrap_or_else(|| ui_path("/login"));

    HttpResponse::Found()
        .cookie(realm::cleared_session_cookie())
        .cookie(realm::cleared_refresh_cookie())
        .append_header(("Location", target))
        .finish()
}

pub fn render_login_page(
    request: &HttpRequest,
    error: bool,
    notices: &LoginNotices,
) -> HttpResponse {
    // Carried through the form so a login that started at sage returns to sage.
    let redirect = redirect_target(request);

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

    // The page shell has no top panel here, so the card carries the estate
    // label and the language switch itself, in a bar above the credentials.
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
        HttpResponse::Ok(),
        content().class("container-fluid login-layout").child(
            div()
                .class("panel login-panel")
                .child(login_bar)
                .child(credentials),
        ),
        UiPageKind::Auth,
    )
}

/// Only these two literal error keys are ever produced by the redirects that
/// land here (`register.rs`'s `verify`, `reset.rs`'s reset submit) - checked
/// against a fixed list rather than trusted from the query string, so a
/// hand-crafted link cannot put arbitrary text on the page.
pub fn login_error_key(notices: &LoginNotices) -> Option<&'static str> {
    match notices.err.as_deref() {
        Some("ui_login_verify_invalid") => Some("ui_login_verify_invalid"),
        Some("ui_login_reset_invalid") => Some("ui_login_reset_invalid"),
        Some("ui_login_account_disabled") => Some("ui_login_account_disabled"),
        Some("ui_login_account_locked") => Some("ui_login_account_locked"),
        _ => None,
    }
}

/// Which "ok" banner to show, checked in an order that matters: a successful
/// reset is more specific news than "we sent a link" would be if somehow both
/// were set.
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

pub fn render_mfa_page(pending: &str, redirect: Option<&str>, error: bool) -> HttpResponse {
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

fn render_auth_page(title_key: &'static str, inner_form: Element) -> HttpResponse {
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
        HttpResponse::Ok(),
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
pub fn login_redirect() -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", ui_path("/login")))
        .finish()
}
