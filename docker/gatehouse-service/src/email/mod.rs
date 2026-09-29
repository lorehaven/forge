//! Email seam: `Sender` is what the rest of gatehouse talks to.
//!
//! `SmtpSender` (see [`smtp`]) delivers through a real mail server;
//! `LoggingSender` writes the link to the log instead, for dev and BDD.
//! [`sender_from_env`] picks one: SMTP when `SMTP_HOST` is set, the log otherwise.

mod budget;
mod smtp;
mod templates;

pub use budget::BudgetedSender;
pub use smtp::{SmtpConfig, SmtpSender, TlsMode};
pub use templates::{Kind, Rendered, language, render};

use crate::ratelimit::{RateLimiter, policy};
use async_trait::async_trait;
use std::fmt;
use std::sync::Arc;

/// Who an email is for.
#[derive(Debug, Clone, Copy)]
pub struct Recipient<'a> {
    pub address: &'a str,
    pub username: &'a str,
    /// A locale tag (`pl-PL`, `de`, ...); anything unknown means English.
    pub locale: Option<&'a str>,
}

/// Why an email was not sent. `is_transient` says whether trying again later
/// could help (network trouble, a busy server) or not (a refused address).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendError {
    message: String,
    transient: bool,
}

impl SendError {
    pub fn new(message: impl Into<String>, transient: bool) -> Self {
        Self {
            message: message.into(),
            transient,
        }
    }

    pub fn transient(message: impl Into<String>) -> Self {
        Self::new(message, true)
    }

    pub fn permanent(message: impl Into<String>) -> Self {
        Self::new(message, false)
    }

    pub fn is_transient(&self) -> bool {
        self.transient
    }
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SendError {}

#[async_trait]
pub trait Sender: Send + Sync {
    async fn send_verification(&self, to: &Recipient<'_>, link: &str) -> Result<(), SendError>;
    async fn send_password_reset(&self, to: &Recipient<'_>, link: &str) -> Result<(), SendError>;
}

/// Logs the link instead of emailing it - dev/BDD only, never a real deployment.
pub struct LoggingSender;

#[async_trait]
impl Sender for LoggingSender {
    async fn send_verification(&self, to: &Recipient<'_>, link: &str) -> Result<(), SendError> {
        tracing::info!(
            "email(verification) to={} user={}: visit {link} to verify this address",
            to.address,
            to.username
        );
        Ok(())
    }

    async fn send_password_reset(&self, to: &Recipient<'_>, link: &str) -> Result<(), SendError> {
        tracing::info!(
            "email(password-reset) to={} user={}: visit {link} to choose a new password",
            to.address,
            to.username
        );
        Ok(())
    }
}

/// `MAIL_DAILY_LIMIT`: how many emails per day the whole estate may send.
/// Unset means the default (a little under the relay's free allowance).
pub fn daily_mail_limit(get: &dyn Fn(&str) -> Option<String>) -> Result<usize, String> {
    match get("MAIL_DAILY_LIMIT").map(|v| v.trim().to_string()) {
        None => Ok(policy::DEFAULT_DAILY_MAIL),
        Some(value) if value.is_empty() => Ok(policy::DEFAULT_DAILY_MAIL),
        Some(value) => value
            .parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| format!("MAIL_DAILY_LIMIT {value:?} is not a positive number")),
    }
}

/// The sender the environment asks for. An invalid mail configuration is a
/// startup failure, like the other required settings - better than a service
/// that comes up and silently cannot send a reset link.
pub fn sender_from_env(limiter: &RateLimiter) -> Arc<dyn Sender> {
    match SmtpConfig::from_env() {
        Ok(Some(config)) => {
            tracing::info!("mail: sending through {config:?}");
            let per_day = daily_mail_limit(&|key| std::env::var(key).ok())
                .unwrap_or_else(|err| panic!("invalid mail configuration: {err}"));
            tracing::info!("mail: at most {per_day} emails per day");
            let sender = SmtpSender::new(config).expect("invalid mail configuration");
            sender.spawn_startup_check();
            Arc::new(BudgetedSender::new(
                Arc::new(sender),
                limiter.clone(),
                per_day,
            ))
        }
        Ok(None) => {
            tracing::warn!(
                "mail: SMTP_HOST is not set - verification and reset links are written to the log, not emailed"
            );
            Arc::new(LoggingSender)
        }
        Err(err) => panic!("invalid mail configuration: {err}"),
    }
}
