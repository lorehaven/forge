//! Password reset by email - two public pages, request then use the link.

use crate::PublicBase;
use crate::email::{self, Recipient};
use crate::ratelimit::{ClientIp, RateLimiter, policy};
use crate::realm;
use crate::tokens::{PURPOSE_RESET_PASSWORD, VerificationTokens};
use crate::ui::common::{UiPageKind, render_page, supported_locales, ui_path};
use crate::ui::locale::BrowserLocale;
use http::StatusCode;
use quench_auth::domain::session::SessionDb;
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Query, Response, get, post};
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

/// Shorter than a verification link - a reset link changes the password outright.
pub const RESET_TTL_SECS: u64 = 60 * 60;

#[derive(Deserialize)]
pub struct ForgotPasswordForm {
    pub username: String,
}

#[get("/ui/forgot-password")]
pub async fn forgot_password_page() -> Response {
    render_forgot_password_page()
}

#[get("/ui/forgot-password/")]
pub async fn forgot_password_page_slash() -> Response {
    render_forgot_password_page()
}

#[post("/ui/forgot-password")]
// One DI extractor per argument - the framework has no struct-of-extractors.
#[allow(clippy::too_many_arguments)]
pub async fn forgot_password_submit(
    Form(form): Form<ForgotPasswordForm>,
    Inject(db): Inject<Db>,
    Inject(mailer): Inject<Arc<dyn email::Sender>>,
    Inject(base): Inject<PublicBase>,
    browser_locale: BrowserLocale,
    ip: ClientIp,
    Inject(limiter): Inject<RateLimiter>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
    // Counted by what was asked (client, submitted name), never by whether the
    // account exists - so being refused reveals nothing about it, and a refusal
    // is the one outcome allowed to differ from the redirect below.
    let verdict = limiter
        .check_all(&[
            ("reset-ip", &ip.0, policy::RESET_IP),
            ("reset-user", &form.username, policy::RESET_USER),
        ])
        .await;
    if !verdict.is_allowed() {
        return redirect(&ui_path("/login?err=ui_login_rate_limited"));
    }

    // Same redirect regardless of outcome - the caller doesn't get to learn why.
    if let Ok(user) = realm::get(&db, &form.username).await
        && let Some(email) = &user.email
    {
        match tokens
            .issue(PURPOSE_RESET_PASSWORD, &user.username, RESET_TTL_SECS)
            .await
        {
            Ok(token) => {
                let link = format!(
                    "{}{}",
                    base.as_str(),
                    ui_path(&format!("/reset-password?token={token}"))
                );
                let recipient = Recipient {
                    address: email,
                    username: &user.username,
                    locale: user
                        .preferred_locale
                        .as_deref()
                        .or(browser_locale.0.as_deref()),
                };
                if let Err(err) = mailer.send_password_reset(&recipient, &link).await {
                    // Same redirect either way - the caller must not learn whether
                    // the account exists, or whether mail is working.
                    tracing::error!(
                        "failed to send the password reset email for {}: {err}",
                        user.username
                    );
                }
            }
            Err(err) => {
                tracing::error!(
                    "failed to issue a password reset token for {}: {err}",
                    user.username
                );
            }
        }
    }

    redirect(&ui_path("/login?reset_requested=1"))
}

#[derive(Deserialize)]
pub struct ResetPasswordQuery {
    pub token: String,
}

#[derive(Deserialize, Default)]
pub struct ResetNotice {
    #[serde(default)]
    pub err: Option<String>,
}

#[get("/ui/reset-password")]
pub async fn reset_password_page(
    Query(query): Query<ResetPasswordQuery>,
    Query(notice): Query<ResetNotice>,
) -> Response {
    render_reset_password_page(&query.token, &notice)
}

#[derive(Deserialize)]
pub struct ResetPasswordForm {
    pub token: String,
    pub password: String,
}

#[post("/ui/reset-password")]
pub async fn reset_password_submit(
    Form(form): Form<ResetPasswordForm>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
    let Some(username) = tokens
        .redeem(PURPOSE_RESET_PASSWORD, &form.token)
        .await
        .unwrap_or(None)
    else {
        return redirect(&ui_path("/login?err=ui_login_reset_invalid"));
    };

    match realm::reset_password(&db, &sessions, &username, &form.password).await {
        Ok(()) => redirect(&ui_path("/login?reset=1")),
        Err(_) => redirect(&format!(
            "{}?token={}&err=ui_reset_error_password_empty",
            ui_path("/reset-password"),
            urlencoding::encode(&form.token)
        )),
    }
}

pub fn render_forgot_password_page() -> Response {
    let request_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/forgot-password"))
        .child(
            label()
                .attr("for", "username")
                .attr("data-i18n", "ui_login_username"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "username")
                .attr("name", "username")
                .attr("autocomplete", "username")
                .attr("autofocus", "autofocus")
                .attr("required", "required"),
        )
        .child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_forgot_password_hint"),
        )
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_forgot_password_submit"),
        );

    render_auth_page("ui_forgot_password_title", request_form)
}

pub fn render_reset_password_page(token: &str, notice: &ResetNotice) -> Response {
    let mut reset_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/reset-password"))
        .child(
            element("input")
                .attr("type", "hidden")
                .attr("name", "token")
                .attr("value", token),
        )
        .child(
            label()
                .attr("for", "password")
                .attr("data-i18n", "ui_reset_new_password"),
        )
        .child(
            input()
                .attr("type", "password")
                .attr("id", "password")
                .attr("name", "password")
                .attr("autocomplete", "new-password")
                .attr("autofocus", "autofocus")
                .attr("required", "required"),
        )
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_reset_submit"),
        );

    if notice.err.as_deref() == Some("ui_reset_error_password_empty") {
        reset_form = reset_form.child(
            p().class("error")
                .attr("data-i18n", "ui_reset_error_password_empty"),
        );
    }

    render_auth_page("ui_reset_title", reset_form)
}

pub(super) fn render_auth_page(title_key: &'static str, inner_form: Element) -> Response {
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

fn redirect(path: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", path)
}

pub fn register_routes() {
    let _ = forgot_password_page as fn() -> _;
    let _ = forgot_password_page_slash as fn() -> _;
    let _ = forgot_password_submit as fn(_, _, _, _, _, _, _, _) -> _;
    let _ = reset_password_page as fn(_, _) -> _;
    let _ = reset_password_submit as fn(_, _, _, _) -> _;
}
