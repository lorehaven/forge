//! Shared test-only support used across this crate's `tests/unit/` modules.
//!
//! Every test file that exercises this crate compiles into a separate
//! `tests/unit.rs` binary but they all share one process environment, so any
//! test that mutates an env var with a fixed name (rather than a name it
//! invents for itself) needs to coordinate with every other module that also
//! touches that name - a lock private to one module protects nothing against
//! a test in a different module racing it. This module is the one place that
//! coordination lives, so it doesn't get reinvented (differently, and
//! incompatibly) per module. It has no callers outside `tests/`, but must be
//! `pub` (not `pub(crate)`) for the separate `tests/unit.rs` binary to reach
//! it - hence the `dead_code` allowance for ordinary (non-test) builds.
#![allow(dead_code)]

use std::sync::OnceLock;
use tokio::sync::{Mutex, MutexGuard};

/// Guards `SERVICE_AUTH_ENABLED`, read by `JwtConfig`/`SubjectClaims` and
/// toggled by `ui::tests` and `api::users::tests`.
pub fn service_auth_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Holds [`service_auth_env_lock`] and pins `SERVICE_AUTH_ENABLED` to
/// "false" for the guard's lifetime.
///
/// Every module in this binary that builds a `JwtConfig::for_tests()` and
/// relies on its bypass identity (auth disabled by default) must hold this
/// guard, not just the modules that flip the var to "true" - the lock only
/// protects against a race if every reader of the assumed default also
/// takes it. Without this, a test elsewhere in the same process (they all
/// share one env) can flip `SERVICE_AUTH_ENABLED` to "true" mid-request and
/// turn an expected 200 into a login redirect at random.
pub async fn auth_disabled_guard() -> MutexGuard<'static, ()> {
    let guard = service_auth_env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };
    guard
}

/// `GATEHOUSE_KEY_ENCRYPTION_KEY` is a fixed env var name read by several
/// modules' tests in this crate (`crypto`, `keys`, `mfa`, `realm`,
/// `api::jwks`), all compiled into one `tests/unit.rs` binary that runs them
/// in parallel. Every one of them sets it to this exact same value via
/// `envmnt::set` and never unsets it - a set-only, same-value convention
/// rather than set-then-remove, because concurrent writes of an identical
/// value race harmlessly while set/remove across different modules does not:
/// a per-module lock (or per-module differing value) cannot stop one
/// module's "remove" from yanking the var out from under another module's
/// read of it.
pub const TEST_KEY_MATERIAL: &str = "test-key-material";
