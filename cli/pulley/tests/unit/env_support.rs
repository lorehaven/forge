//! Shared across every test in this binary that mutates process-global state
//! (`HOME`/`USERPROFILE`/`USER`/`SUDO_USER` env vars, or the current
//! directory) - cargo runs all tests in this binary in parallel by default,
//! so any test that touches one of these must hold `ENV_LOCK` for as long as
//! the mutation matters to it.

use std::sync::Mutex;

pub static ENV_LOCK: Mutex<()> = Mutex::new(());

pub struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    pub fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        // SAFETY: serialized by ENV_LOCK, held for the guard's lifetime.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }

    pub fn unset(key: &'static str) -> Self {
        let previous = std::env::var(key).ok();
        // SAFETY: serialized by ENV_LOCK, held for the guard's lifetime.
        unsafe { std::env::remove_var(key) };
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: serialized by ENV_LOCK, held for the guard's lifetime.
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}
