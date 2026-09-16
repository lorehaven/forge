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
