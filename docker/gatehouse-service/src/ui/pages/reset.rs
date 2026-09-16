//! Password reset by email - two public pages, request then use the link.

use crate::email;
use crate::realm;
use crate::tokens::{PURPOSE_RESET_PASSWORD, VerificationTokens};
use crate::ui::common::{UiPageKind, render_page, supported_locales, ui_path};
use async_trait::async_trait;
use http::StatusCode;
use quench_auth::domain::session::SessionDb;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Form, FromRequest, HttpError, Inject, Query, Request, Response, get, post,
};
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

/// Shorter than a verification link - a reset link changes the password outright.
const RESET_TTL_SECS: u64 = 60 * 60;

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

pub struct AbsoluteBase(String);

#[async_trait]
impl FromRequest for AbsoluteBase {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let scheme = match req.header("x-forwarded-proto") {
            Some(scheme) => scheme.to_string(),
            None => req
                .container()
                .get::<crate::ui::common::ExternalScheme>()
                .map(|s| s.0.to_string())
                .unwrap_or_else(|_| "https".to_string()),
        };
        let host = req
            .header("x-forwarded-host")
            .or_else(|| req.header("host"))
            .unwrap_or("");
        Ok(Self(format!("{scheme}://{host}")))
    }
}

#[post("/ui/forgot-password")]
pub async fn forgot_password_submit(
    base: AbsoluteBase,
    Form(form): Form<ForgotPasswordForm>,
    Inject(db): Inject<Db>,
    Inject(mailer): Inject<Arc<dyn email::Sender>>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
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
                    base.0,
                    ui_path(&format!("/reset-password?token={token}"))
                );
                mailer
                    .send_password_reset(email, &user.username, &link)
                    .await;
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

fn redirect(path: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", path)
}

pub fn register_routes() {
    let _ = forgot_password_page as fn() -> _;
    let _ = forgot_password_page_slash as fn() -> _;
    let _ = forgot_password_submit as fn(_, _, _, _, _) -> _;
    let _ = reset_password_page as fn(_, _) -> _;
    let _ = reset_password_submit as fn(_, _, _, _) -> _;
}
