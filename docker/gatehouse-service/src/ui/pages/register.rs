//! Self-service registration.
//!
//! Public, like the login page next door - the account does not exist yet, so
//! there is nothing to authenticate against. Creating the account and sending
//! the verification link both go through the same primitives the admin pages
//! use (`crate::realm`, `crate::tokens`), so "a new user starts with the
//! catalog's default template" and "a verification link is single-use" are
//! each enforced in one place, not re-implemented here.

use crate::catalog::PermissionCatalog;
use crate::email;
use crate::realm::{self, RealmError};
use crate::tokens::{PURPOSE_VERIFY_EMAIL, VerificationTokens};
use crate::ui::common::{UiPageKind, render_page, supported_locales, ui_path};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

/// A verification link is good for a day. Long enough that "I'll get to it
/// later" still works; short enough that a link sitting in an old email
/// unread for months is not a live credential forever.
const VERIFICATION_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub password: String,
    pub email: String,
}

#[get("/register")]
pub(super) async fn register_page(query: web::Query<Notice>) -> impl Responder {
    render_register_page(&query)
}

#[get("/register/")]
pub(super) async fn register_page_slash(query: web::Query<Notice>) -> impl Responder {
    render_register_page(&query)
}

#[derive(Deserialize, Default)]
pub struct Notice {
    #[serde(default)]
    pub err: Option<String>,
}

#[post("/register")]
pub(super) async fn register_submit(
    request: HttpRequest,
    form: web::Form<RegisterForm>,
    catalog: web::Data<PermissionCatalog>,
    db: web::Data<quench_db::prelude::Db>,
    mailer: web::Data<Arc<dyn email::Sender>>,
    tokens: web::Data<Arc<VerificationTokens>>,
) -> impl Responder {
    let form = form.into_inner();

    if form.email.trim().is_empty() || !form.email.contains('@') {
        return redirect(&ui_path("/register?err=ui_register_error_email_invalid"));
    }

    let user = match realm::register(&db, &catalog, &form.username, &form.password, &form.email)
        .await
    {
        Ok(user) => user,
        Err(err) => return redirect(&format!("{}?err={}", ui_path("/register"), err.i18n_key())),
    };

    match tokens
        .issue(PURPOSE_VERIFY_EMAIL, &user.username, VERIFICATION_TTL_SECS)
        .await
    {
        Ok(token) => {
            let link = absolute_url(&request, &format!("/verify?token={token}"));
            mailer
                .send_verification(&form.email, &user.username, &link)
                .await;
        }
        Err(err) => {
            // The account exists either way - a token failure should not look
            // like registration itself failed, since retrying would just hit
            // "username already taken". Logged loudly because it means nobody
            // can verify this address until it is fixed.
            tracing::error!(
                "failed to issue a verification token for {}: {err}",
                user.username
            );
        }
    }

    redirect(&ui_path("/login?registered=1"))
}

#[get("/verify")]
pub(super) async fn verify(
    query: web::Query<VerifyQuery>,
    db: web::Data<quench_db::prelude::Db>,
    tokens: web::Data<Arc<VerificationTokens>>,
) -> impl Responder {
    let Some(username) = tokens
        .redeem(PURPOSE_VERIFY_EMAIL, &query.token)
        .await
        .unwrap_or(None)
    else {
        return redirect(&ui_path("/login?err=ui_login_verify_invalid"));
    };

    match realm::mark_email_verified(&db, &username).await {
        Ok(()) => redirect(&ui_path("/login?verified=1")),
        Err(err) => {
            tracing::error!("failed to record email verification for {username}: {err:?}");
            redirect(&ui_path("/login?err=ui_login_verify_invalid"))
        }
    }
}

#[derive(Deserialize)]
pub struct VerifyQuery {
    pub token: String,
}

/// Best-effort absolute URL for `path` on this service, so a link handed to
/// an email client - which has no notion of "the current origin" to resolve a
/// relative one against - actually goes somewhere.
fn absolute_url(request: &HttpRequest, path: &str) -> String {
    let info = request.connection_info().clone();
    format!("{}://{}{}", info.scheme(), info.host(), ui_path(path))
}

fn render_register_page(notice: &Notice) -> HttpResponse {
    let mut register_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/register"))
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
            label()
                .attr("for", "email")
                .attr("data-i18n", "ui_register_email"),
        )
        .child(
            input()
                .attr("type", "email")
                .attr("id", "email")
                .attr("name", "email")
                .attr("autocomplete", "email")
                .attr("required", "required"),
        )
        .child(
            label()
                .attr("for", "password")
                .attr("data-i18n", "ui_login_password"),
        )
        .child(
            input()
                .attr("type", "password")
                .attr("id", "password")
                .attr("name", "password")
                .attr("autocomplete", "new-password")
                .attr("required", "required"),
        )
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_register_submit"),
        );

    if let Some(key) = notice.err.as_deref().and_then(known_error_key) {
        register_form = register_form.child(p().class("error").attr("data-i18n", key));
    }

    let register_bar = div()
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
                .attr("data-i18n", "ui_register_title"),
        )
        .child(div().class("meta-list").child(register_form))
        .child(
            a().class("admin-hint")
                .attr("href", ui_path("/login"))
                .attr("data-i18n", "ui_register_have_account"),
        );

    render_page(
        HttpResponse::Ok(),
        content().class("container-fluid login-layout").child(
            div()
                .class("panel login-panel")
                .child(register_bar)
                .child(credentials),
        ),
        UiPageKind::Auth,
    )
}

/// Only keys `RealmError` or this page's own validation can produce are
/// rendered - a hand-crafted `?err=` cannot put arbitrary text on the page.
fn known_error_key(candidate: &str) -> Option<&'static str> {
    if candidate == "ui_register_error_email_invalid" {
        return Some("ui_register_error_email_invalid");
    }
    [
        RealmError::UsernameEmpty,
        RealmError::PasswordEmpty,
        RealmError::AlreadyExists,
        RealmError::Internal,
    ]
    .iter()
    .map(RealmError::i18n_key)
    .find(|known| *known == candidate)
}

fn redirect(path: &str) -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", path.to_string()))
        .finish()
}
