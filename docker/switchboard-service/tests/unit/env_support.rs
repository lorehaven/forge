//! One shared lock for every test in this binary that mutates
//! `SERVICE_AUTH_ENABLED` (or any other ambient env var read by request
//! handlers). All `tests/unit/*.rs` files compile into the same `unit` test
//! binary and therefore share one process's environment, so a per-file lock
//! only serializes within that file - it does nothing to stop a test in one
//! file racing a test in another. Every test that reads or writes an env var
//! shared handlers depend on must hold this lock for the duration.

use std::sync::OnceLock;
use tokio::sync::Mutex;

/// `tokio::sync::Mutex`, not `std::sync::Mutex`: every holder here keeps the
/// guard across an `.await` (an HTTP call, a DB round-trip), and only ever
/// from within `#[tokio::test]`/`#[actix_web::test]` bodies, so there's no
/// blocking-executor concern - just the ordinary async lock for the job.
pub fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// `routers_models_handlers_tests` shares one process-global `MODEL_STORE`
/// (see that file's module docs). `sync_models` wipes every entry that isn't
/// on disk - which, in this sandbox, is all of them - so any test that
/// inserts a model and then expects to read it back must hold this lock for
/// as long as that expectation needs to stay true, and `sync_models` itself
/// must hold it for its whole run.
pub fn store_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// `MockVllmEngine` keeps its instances in a process-global static keyed by
/// `mock-{millisecond timestamp}`. Two tests launching within the same
/// millisecond would collide on that key, so every test that launches or
/// inspects a mock instance holds this lock for the duration.
pub fn mock_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
