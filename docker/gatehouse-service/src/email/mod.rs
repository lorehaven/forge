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
pub use templates::{
    Kind, Rendered, Vars, fill_with, html as render_html, language, render, render_with,
};

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

/// Everything gatehouse can email. A new kind of message is a new variant here,
/// a template in [`templates`], and nothing in the transports.
#[derive(Debug, Clone, Copy)]
pub enum Mail<'a> {
    Verification {
        link: &'a str,
    },
    PasswordReset {
        link: &'a str,
    },
    Invite {
        link: &'a str,
    },
    /// To the NEW address.
    EmailChange {
        link: &'a str,
    },
    /// To the OLD address, after the change.
    EmailChanged {
        new_email: &'a str,
    },
    PasswordChanged,
    MfaEnabled,
    MfaDisabled,
    /// Something another service asked gatehouse to tell the recipient (see
    /// `crate::notify`). `unsubscribe` is the link that stops this kind of message.
    Notification {
        template: &'a str,
        vars: &'a [(&'a str, &'a str)],
        unsubscribe: &'a str,
    },
}

impl<'a> Mail<'a> {
    pub fn kind(&self) -> Kind {
        match self {
            Mail::Verification { .. } => Kind::Verification,
            Mail::PasswordReset { .. } => Kind::PasswordReset,
            Mail::Invite { .. } => Kind::Invite,
            Mail::EmailChange { .. } => Kind::EmailChange,
            Mail::EmailChanged { .. } => Kind::EmailChanged,
            Mail::PasswordChanged => Kind::PasswordChanged,
            Mail::MfaEnabled => Kind::MfaEnabled,
            Mail::MfaDisabled => Kind::MfaDisabled,
            Mail::Notification { .. } => Kind::Notification,
        }
    }

    /// The link the message carries, empty for notices.
    pub fn link(&self) -> &'a str {
        match self {
            Mail::Verification { link }
            | Mail::PasswordReset { link }
            | Mail::Invite { link }
            | Mail::EmailChange { link } => link,
            Mail::Notification { unsubscribe, .. } => unsubscribe,
            _ => "",
        }
    }

    /// What else identifies the message: the address a change notice names, or a
    /// notification's template id. Empty when there is nothing.
    pub fn detail(&self) -> &'a str {
        match self {
            Mail::EmailChanged { new_email } => new_email,
            Mail::Notification { template, .. } => template,
            _ => "",
        }
    }

    /// The address named in the message, empty when it names none.
    pub fn new_email(&self) -> &'a str {
        match self {
            Mail::EmailChanged { new_email } => new_email,
            _ => "",
        }
    }

    pub fn vars(&self, username: &'a str) -> Vars<'a> {
        Vars {
            username,
            link: self.link(),
            new_email: self.new_email(),
        }
    }
}

/// What the rest of gatehouse talks to. Implementors provide [`send`](Self::send);
/// the two named helpers are the original entry points, kept for callers.
#[async_trait]
pub trait Sender: Send + Sync {
    async fn send(&self, to: &Recipient<'_>, mail: &Mail<'_>) -> Result<(), SendError>;

    async fn send_verification(&self, to: &Recipient<'_>, link: &str) -> Result<(), SendError> {
        self.send(to, &Mail::Verification { link }).await
    }

    async fn send_password_reset(&self, to: &Recipient<'_>, link: &str) -> Result<(), SendError> {
        self.send(to, &Mail::PasswordReset { link }).await
    }
}

/// Logs the message instead of emailing it - dev/BDD only, never a real
/// deployment. The `visit <link> to ...` shape of the verification and reset
/// lines is what the BDD suite reads the links back from.
pub struct LoggingSender;

#[async_trait]
impl Sender for LoggingSender {
    async fn send(&self, to: &Recipient<'_>, mail: &Mail<'_>) -> Result<(), SendError> {
        let kind = mail.kind().label();
        let (address, user) = (to.address, to.username);
        match mail {
            Mail::Verification { link } => tracing::info!(
                "email({kind}) to={address} user={user}: visit {link} to verify this address"
            ),
            Mail::PasswordReset { link } => tracing::info!(
                "email({kind}) to={address} user={user}: visit {link} to choose a new password"
            ),
            Mail::Invite { link } => tracing::info!(
                "email({kind}) to={address} user={user}: visit {link} to accept the invitation"
            ),
            Mail::EmailChange { link } => tracing::info!(
                "email({kind}) to={address} user={user}: visit {link} to confirm the new address"
            ),
            Mail::EmailChanged { new_email } => tracing::info!(
                "email({kind}) to={address} user={user}: the address was changed to {new_email}"
            ),
            Mail::Notification {
                template,
                unsubscribe,
                ..
            } => tracing::info!(
                "email({kind}) to={address} user={user}: {template} (unsubscribe: {unsubscribe})"
            ),
            Mail::PasswordChanged | Mail::MfaEnabled | Mail::MfaDisabled => {
                tracing::info!("email({kind}) to={address} user={user}")
            }
        }
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
