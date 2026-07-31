//! Sending mail.
//!
//! There is no email transport anywhere in this estate yet, and building one
//! (a provider, credentials, retries, bounce handling) is a separate piece of
//! work from wiring registration and password reset up to *use* email. This is
//! the seam: a `Sender` trait with exactly the two messages gatehouse needs to
//! send, and one implementation, `LoggingSender`, which writes the link to the
//! log instead of anywhere the recipient would see it. Dev-only, deliberately -
//! it is what makes registration and password reset testable today without an
//! SMTP relay, and it is the thing a real provider replaces without anything
//! above this module changing.

use async_trait::async_trait;

#[async_trait]
pub trait Sender: Send + Sync {
    async fn send_verification(&self, to: &str, username: &str, link: &str);
    async fn send_password_reset(&self, to: &str, username: &str, link: &str);
}

/// Logs the link at `info` instead of emailing it anywhere.
///
/// Never wire this in for a deployment a real user reaches - the link is a
/// credential (it proves control of the account) and this prints it to
/// whatever is reading the process's logs. It exists for local development
/// and for the BDD suite, both of which already treat the log as the source
/// of truth for what the service just did.
pub struct LoggingSender;

#[async_trait]
impl Sender for LoggingSender {
    async fn send_verification(&self, to: &str, username: &str, link: &str) {
        tracing::info!(
            "email(verification) to={to} user={username}: visit {link} to verify this address"
        );
    }

    async fn send_password_reset(&self, to: &str, username: &str, link: &str) {
        tracing::info!(
            "email(password-reset) to={to} user={username}: visit {link} to choose a new password"
        );
    }
}
