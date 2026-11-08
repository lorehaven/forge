//! Shared across every test in this binary that mutates process-global state
//! (`HOME`/`USERPROFILE`/`USER`/`SUDO_USER` env vars, or the current
//! directory) - cargo runs all tests in this binary in parallel by default,
//! so any test that touches one of these must hold `ENV_LOCK` for as long as
//! the mutation matters to it.

use std::sync::Mutex;

pub static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Goes through `quench_core::env` rather than `std::env::set_var` directly since this crate forbids
/// `unsafe_code`.
#[derive(Debug)]
pub struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    pub fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        // Serialized by ENV_LOCK, held for the guard's lifetime.
        quench_core::env::set(key, value);
        Self { key, previous }
    }

    pub fn unset(key: &'static str) -> Self {
        let previous = std::env::var(key).ok();
        // Serialized by ENV_LOCK, held for the guard's lifetime.
        quench_core::env::remove(key);
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // Serialized by ENV_LOCK, held for the guard's lifetime.
        match &self.previous {
            Some(value) => quench_core::env::set(self.key, value),
            None => quench_core::env::remove(self.key),
        }
    }
}
