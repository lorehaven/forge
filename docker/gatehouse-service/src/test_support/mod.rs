//! Shared env-var-locking support for `tests/unit/` - all test modules share
//! one process, so fixed-name env var mutations must coordinate here.
#![allow(dead_code)]

use std::sync::OnceLock;
use tokio::sync::{Mutex, MutexGuard};

/// Guards `SERVICE_AUTH_ENABLED`, read by `JwtConfig`/`SubjectClaims` and
/// toggled by `ui::tests` and `api::users::tests`.
pub fn service_auth_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Pins `SERVICE_AUTH_ENABLED` to "false"; every test relying on that default must hold this too.
pub async fn auth_disabled_guard() -> MutexGuard<'static, ()> {
    let guard = service_auth_env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };
    guard
}

/// Set-only convention (never unset) for `GATEHOUSE_KEY_ENCRYPTION_KEY` -
/// concurrent identical writes race harmlessly, set/remove does not.
pub const TEST_KEY_MATERIAL: &str = "test-key-material";

/// One message a [`RecordingSender`] was asked to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentMail {
    pub kind: &'static str,
    pub to: String,
    pub username: String,
    pub locale: Option<String>,
    pub link: String,
}

/// Captures what would have been emailed, so a test can assert on the link
/// instead of scraping logs.
#[derive(Default)]
pub struct RecordingSender {
    sent: std::sync::Mutex<Vec<SentMail>>,
}

impl RecordingSender {
    pub fn sent(&self) -> Vec<SentMail> {
        self.sent.lock().expect("recording lock").clone()
    }

    fn record(&self, kind: &'static str, to: &crate::email::Recipient<'_>, link: &str) {
        self.sent.lock().expect("recording lock").push(SentMail {
            kind,
            to: to.address.to_string(),
            username: to.username.to_string(),
            locale: to.locale.map(str::to_string),
            link: link.to_string(),
        });
    }
}

#[async_trait::async_trait]
impl crate::email::Sender for RecordingSender {
    async fn send_verification(
        &self,
        to: &crate::email::Recipient<'_>,
        link: &str,
    ) -> Result<(), crate::email::SendError> {
        self.record("verification", to, link);
        Ok(())
    }

    async fn send_password_reset(
        &self,
        to: &crate::email::Recipient<'_>,
        link: &str,
    ) -> Result<(), crate::email::SendError> {
        self.record("password-reset", to, link);
        Ok(())
    }
}
