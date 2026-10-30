//! Password reset by email - two public pages, request then use the link.

use crate::PublicBase;
use crate::email::{self, Mail, Recipient};
use crate::notices;
use crate::ratelimit::{ClientIp, RateLimiter, policy};
use crate::realm;
use crate::tokens::{PURPOSE_RESET_PASSWORD, VerificationTokens};
use crate::ui::common::ui_path;
use crate::ui::locale::BrowserLocale;
use quench_auth::domain::session::SessionDb;
use quench_db::prelude::Db;
use quench_http::http::StatusCode;
use quench_http::prelude::{Form, Inject, Query, Response, get, post};
use quench_serde::Deserialize;
use std::sync::Arc;

/// Shorter than a verification link - a reset link changes the password outright.
pub const RESET_TTL_SECS: u64 = 60 * 60;

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordForm {
    pub username: String,
}

#[get("/ui/forgot-password")]
pub async fn forgot_password_page() -> Response {
    super::client::public()
}

#[get("/ui/forgot-password/")]
pub async fn forgot_password_page_slash() -> Response {
    super::client::public()
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
                    quench_log::error!(
                        "failed to send the password reset email for {}: {err}",
                        user.username
                    );
                }
            }
            Err(err) => {
                quench_log::error!(
                    "failed to issue a password reset token for {}: {err}",
                    user.username
                );
            }
        }
    }

    redirect(&ui_path("/login?reset_requested=1"))
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordQuery {
    pub token: String,
}

/// The page the emailed link opens: the client draws the form for the `token` in the address.
#[get("/ui/reset-password")]
pub async fn reset_password_page(Query(_query): Query<ResetPasswordQuery>) -> Response {
    super::client::public()
}

#[derive(Debug, Deserialize)]
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
    Inject(mailer): Inject<Arc<dyn email::Sender>>,
    browser_locale: BrowserLocale,
) -> Response {
    let Some(username) = tokens
        .redeem(PURPOSE_RESET_PASSWORD, &form.token)
        .await
        .unwrap_or(None)
    else {
        return redirect(&ui_path("/login?err=ui_login_reset_invalid"));
    };

    match realm::reset_password(&db, &sessions, &username, &form.password).await {
        Ok(()) => {
            // The link went to the address on file, so following it shows the
            // address is theirs - an unconfirmed one is confirmed by this.
            if let Ok(user) = realm::get(&db, &username).await
                && user.email.is_some()
                && user.email_verified_at.is_none()
                && let Err(err) = realm::mark_email_verified(&db, &username).await
            {
                quench_log::error!(
                    "could not confirm the address of {username} after a reset: {err:?}"
                );
            }
            notices::notify_username(
                &**mailer,
                &db,
                &username,
                &Mail::PasswordChanged,
                browser_locale.0.as_deref(),
            )
            .await;
            redirect(&ui_path("/login?reset=1"))
        }
        Err(_) => redirect(&format!(
            "{}?token={}&err=ui_reset_error_password_empty",
            ui_path("/reset-password"),
            quench_serde::urlencoded::percent_encode(&form.token)
        )),
    }
}

fn redirect(path: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", path)
}

pub fn register_routes() {
    let _ = forgot_password_page as fn() -> _;
    let _ = forgot_password_page_slash as fn() -> _;
    let _ = forgot_password_submit as fn(_, _, _, _, _, _, _, _) -> _;
    let _ = reset_password_page as fn(_) -> _;
    let _ = reset_password_submit as fn(_, _, _, _, _, _) -> _;
}
