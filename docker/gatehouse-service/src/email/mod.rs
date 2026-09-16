//! Email seam: no real transport yet, just `Sender` + `LoggingSender`, which
//! a real provider replaces without touching anything above this module.

use async_trait::async_trait;

#[async_trait]
pub trait Sender: Send + Sync {
    async fn send_verification(&self, to: &str, username: &str, link: &str);
    async fn send_password_reset(&self, to: &str, username: &str, link: &str);
}

/// Logs the link instead of emailing it - dev/BDD only, never a real deployment.
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
