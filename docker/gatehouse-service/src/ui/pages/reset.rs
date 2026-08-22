//! Password reset by email.
//!
//! Two steps, two pages: request a link (`/forgot-password`), then use it
//! (`/reset-password`). Both public, like login and registration - proving
//! you received the email is what stands in for a password here, which is
//! the whole point of the feature.

use crate::email;
use crate::realm;
use crate::tokens::{PURPOSE_RESET_PASSWORD, VerificationTokens};
use crate::ui::common::{UiPageKind, render_page, supported_locales, ui_path};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

/// Shorter than a verification link (`register.rs`'s `VERIFICATION_TTL_SECS`):
/// a password reset link is a stronger credential while it lives - it changes
/// the password outright rather than just confirming an address - so it
/// should not still work weeks after it was requested and forgotten.
const RESET_TTL_SECS: u64 = 60 * 60;

#[derive(Deserialize)]
pub struct ForgotPasswordForm {
    pub username: String,
}

#[get("/forgot-password")]
pub async fn forgot_password_page() -> impl Responder {
    render_forgot_password_page()
}

#[get("/forgot-password/")]
pub async fn forgot_password_page_slash() -> impl Responder {
    render_forgot_password_page()
}

#[post("/forgot-password")]
pub async fn forgot_password_submit(
    request: HttpRequest,
    form: web::Form<ForgotPasswordForm>,
    db: web::Data<quench_db::prelude::Db>,
    mailer: web::Data<Arc<dyn email::Sender>>,
    tokens: web::Data<Arc<VerificationTokens>>,
) -> impl Responder {
    // Same redirect whether the account exists, has an email on file, or the
    // token/send step fails - which of those happened is not something an
    // unauthenticated caller gets to learn from the response. Real work only
    // happens on the path that can actually send something.
    if let Ok(user) = realm::get(&db, &form.username).await
        && let Some(email) = &user.email
    {
        match tokens
            .issue(PURPOSE_RESET_PASSWORD, &user.username, RESET_TTL_SECS)
            .await
        {
            Ok(token) => {
                let link = absolute_url(&request, &format!("/reset-password?token={token}"));
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

#[get("/reset-password")]
pub async fn reset_password_page(
    query: web::Query<ResetPasswordQuery>,
    notice: web::Query<ResetNotice>,
) -> impl Responder {
    render_reset_password_page(&query.token, &notice)
}

#[derive(Deserialize)]
pub struct ResetPasswordForm {
    pub token: String,
    pub password: String,
}

#[post("/reset-password")]
pub async fn reset_password_submit(
    form: web::Form<ResetPasswordForm>,
    db: web::Data<quench_db::prelude::Db>,
    sessions: web::Data<Arc<quench_auth::prelude::SessionDb>>,
    tokens: web::Data<Arc<VerificationTokens>>,
) -> impl Responder {
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

fn absolute_url(request: &HttpRequest, path: &str) -> String {
    let info = request.connection_info().clone();
    format!("{}://{}{}", info.scheme(), info.host(), ui_path(path))
}

pub fn render_forgot_password_page() -> HttpResponse {
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

pub fn render_reset_password_page(token: &str, notice: &ResetNotice) -> HttpResponse {
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

fn redirect(path: &str) -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", path.to_string()))
        .finish()
}
