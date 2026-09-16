//! Self-service registration - public like login, reuses `crate::realm`/`crate::tokens`.

use crate::catalog::PermissionCatalog;
use crate::email;
use crate::realm::{self, RealmError};
use crate::tokens::{PURPOSE_VERIFY_EMAIL, VerificationTokens};
use crate::ui::common::{UiPageKind, render_page, supported_locales, ui_path};
use async_trait::async_trait;
use http::StatusCode;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Form, FromRequest, HttpError, Inject, Query, Request, Response, get, post,
};
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

/// A verification link is good for a day.
const VERIFICATION_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub password: String,
    pub email: String,
}

#[get("/ui/register")]
pub async fn register_page(Query(query): Query<Notice>) -> Response {
    render_register_page(&query)
}

#[get("/ui/register/")]
pub async fn register_page_slash(Query(query): Query<Notice>) -> Response {
    render_register_page(&query)
}

#[derive(Deserialize, Default)]
pub struct Notice {
    #[serde(default)]
    pub err: Option<String>,
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

#[post("/ui/register")]
pub async fn register_submit(
    base: AbsoluteBase,
    Form(form): Form<RegisterForm>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(mailer): Inject<Arc<dyn email::Sender>>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
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
            let link = format!("{}{}", base.0, ui_path(&format!("/verify?token={token}")));
            mailer
                .send_verification(&form.email, &user.username, &link)
                .await;
        }
        Err(err) => {
            // Account exists either way - don't make this look like registration failed.
            tracing::error!(
                "failed to issue a verification token for {}: {err}",
                user.username
            );
        }
    }

    redirect(&ui_path("/login?registered=1"))
}

#[get("/ui/verify")]
pub async fn verify(
    Query(query): Query<VerifyQuery>,
    Inject(db): Inject<Db>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
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

pub fn render_register_page(notice: &Notice) -> Response {
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
        StatusCode::OK,
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
pub fn known_error_key(candidate: &str) -> Option<&'static str> {
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

fn redirect(path: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", path)
}

pub fn register_routes() {
    let _ = register_page as fn(_) -> _;
    let _ = register_page_slash as fn(_) -> _;
    let _ = register_submit as fn(_, _, _, _, _, _) -> _;
    let _ = verify as fn(_, _, _) -> _;
}
