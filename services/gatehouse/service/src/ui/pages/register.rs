//! Self-service registration - public like login, reuses `crate::realm`/`crate::tokens`.

use crate::PublicBase;
use crate::catalog::PermissionCatalog;
use crate::email::{self, Recipient};
use crate::ratelimit::{ClientIp, RateLimiter, policy};
use crate::realm;
use crate::tokens::{PURPOSE_VERIFY_EMAIL, VerificationTokens};
use crate::ui::common::ui_path;
use crate::ui::locale::BrowserLocale;
use quench_db::prelude::Db;
use quench_http::http::StatusCode;
use quench_http::prelude::{Form, Inject, Query, Response, get, post};
use quench_serde::Deserialize;
use std::sync::Arc;

/// A verification link is good for a day.
pub const VERIFICATION_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub password: String,
    pub email: String,
}

/// The registration page: the client draws the form, it posts to `POST /ui/register`.
#[get("/ui/register")]
pub async fn register_page() -> Response {
    super::client::public()
}

#[get("/ui/register/")]
pub async fn register_page_slash() -> Response {
    super::client::public()
}

#[post("/ui/register")]
// One DI extractor per argument - the framework has no struct-of-extractors.
#[allow(clippy::too_many_arguments)]
pub async fn register_submit(
    Form(form): Form<RegisterForm>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(mailer): Inject<Arc<dyn email::Sender>>,
    Inject(base): Inject<PublicBase>,
    browser_locale: BrowserLocale,
    ip: ClientIp,
    Inject(limiter): Inject<RateLimiter>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
    // The same check the mailer applies, so an address that could never be
    // delivered to is refused here rather than after the account exists.
    let email = form.email.trim();
    if email.parse::<quench_mail::Address>().is_err() {
        return redirect(&ui_path("/register?err=ui_register_error_email_invalid"));
    }

    // Each registration mails a stranger's address, so it is limited per client
    // and per address (one victim cannot be flooded through many usernames).
    let verdict = limiter
        .check_all(&[
            ("register-ip", &ip.0, policy::REGISTER_IP),
            ("register-email", email, policy::REGISTER_EMAIL),
        ])
        .await;
    if !verdict.is_allowed() {
        return redirect(&ui_path("/register?err=ui_register_error_rate_limited"));
    }

    let user = match realm::register(&db, &catalog, &form.username, &form.password, email).await {
        Ok(user) => user,
        Err(err) => return redirect(&format!("{}?err={}", ui_path("/register"), err.i18n_key())),
    };

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
                address: email,
                username: &user.username,
                locale: browser_locale.0.as_deref(),
            };
            if let Err(err) = mailer.send_verification(&recipient, &link).await {
                // The account exists either way; the user can ask for another link.
                quench_log::error!(
                    "failed to send the verification email to {email} for {}: {err}",
                    user.username
                );
            }
        }
        Err(err) => {
            // Account exists either way - don't make this look like registration failed.
            quench_log::error!(
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
            quench_log::error!("failed to record email verification for {username}: {err:?}");
            redirect(&ui_path("/login?err=ui_login_verify_invalid"))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct VerifyQuery {
    pub token: String,
}

fn redirect(path: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", path)
}

pub fn register_routes() {
    let _ = register_page as fn() -> _;
    let _ = register_page_slash as fn() -> _;
    let _ = register_submit as fn(_, _, _, _, _, _, _, _, _) -> _;
    let _ = verify as fn(_, _, _) -> _;
}
