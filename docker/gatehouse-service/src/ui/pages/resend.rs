//! Asking for a fresh verification link, for an account whose address is still
//! unconfirmed - the original email may be lost, expired, or never have arrived.
//!
//! Like password reset, the answer never depends on the account: the same
//! redirect whether it exists, is already verified, or has no address.

use super::register::VERIFICATION_TTL_SECS;
use super::reset::render_auth_page;
use crate::PublicBase;
use crate::email::{self, Recipient};
use crate::ratelimit::{ClientIp, RateLimiter, policy};
use crate::realm;
use crate::tokens::{PURPOSE_VERIFY_EMAIL, VerificationTokens};
use crate::ui::common::ui_path;
use crate::ui::locale::BrowserLocale;
use http::StatusCode;
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Response, get, post};
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ResendForm {
    pub username: String,
}

#[get("/ui/resend-verification")]
pub async fn resend_page() -> Response {
    render_resend_page()
}

#[get("/ui/resend-verification/")]
pub async fn resend_page_slash() -> Response {
    render_resend_page()
}

#[post("/ui/resend-verification")]
// One DI extractor per argument - the framework has no struct-of-extractors.
#[allow(clippy::too_many_arguments)]
pub async fn resend_submit(
    Form(form): Form<ResendForm>,
    Inject(db): Inject<Db>,
    Inject(mailer): Inject<Arc<dyn email::Sender>>,
    Inject(base): Inject<PublicBase>,
    browser_locale: BrowserLocale,
    ip: ClientIp,
    Inject(limiter): Inject<RateLimiter>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
    // Counted by what was asked, not by whether the account exists, so being
    // refused tells the caller nothing about it.
    let verdict = limiter
        .check_all(&[
            ("resend-ip", &ip.0, policy::RESEND_IP),
            ("resend-user", &form.username, policy::RESEND_USER),
            ("resend-cooldown", &form.username, policy::RESEND_COOLDOWN),
        ])
        .await;
    if !verdict.is_allowed() {
        return redirect(&ui_path("/login?err=ui_login_rate_limited"));
    }

    if let Ok(user) = realm::get(&db, &form.username).await
        && !user.is_disabled()
        && realm::requires_email_verification(&user)
        && let Some(address) = &user.email
    {
        match tokens
            .issue(PURPOSE_VERIFY_EMAIL, &user.username, VERIFICATION_TTL_SECS)
            .await
        {
            Ok(token) => {
                let link = format!(
                    "{}{}",
                    base.as_str(),
                    ui_path(&format!("/verify?token={token}"))
                );
                let recipient = Recipient {
                    address,
                    username: &user.username,
                    locale: user
                        .preferred_locale
                        .as_deref()
                        .or(browser_locale.0.as_deref()),
                };
                if let Err(err) = mailer.send_verification(&recipient, &link).await {
                    tracing::error!(
                        "failed to resend the verification email for {}: {err}",
                        user.username
                    );
                }
            }
            Err(err) => tracing::error!(
                "failed to issue a verification token for {}: {err}",
                user.username
            ),
        }
    }

    redirect(&ui_path("/login?resend_requested=1"))
}

fn redirect(location: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", location)
}

pub fn render_resend_page() -> Response {
    let request_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/resend-verification"))
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
        .child(p().class("admin-hint").attr("data-i18n", "ui_resend_hint"))
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_resend_submit"),
        );

    render_auth_page("ui_resend_title", request_form)
}

pub fn register_routes() {
    let _ = resend_page as fn() -> _;
    let _ = resend_page_slash as fn() -> _;
    let _ = resend_submit as fn(_, _, _, _, _, _, _, _) -> _;
}
