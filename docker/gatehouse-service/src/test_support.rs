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
use tokio::sync::Mutex;

/// Guards `SERVICE_AUTH_ENABLED`, read by `JwtConfig`/`SubjectClaims` and
/// toggled by `ui::tests` and `api::users::tests`.
pub fn service_auth_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
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
