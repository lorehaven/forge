//! Shared across every test binary in this crate's `tests/` suite that needs
//! `std::process::Command`'s cwd-relative resolution to be stable. Real
//! `cargo metadata`/`git log` calls with no explicit `--manifest-path`, or
//! `std::env::current_dir()` reads that expect to see this crate's directory,
//! must hold `stable_cwd_lock()` for the duration it cares about, and any
//! test that actually calls `std::env::set_current_dir` (cwd is
//! process-global) must hold it for as long as the cwd is changed. The same
//! lock also covers `CARGO_HOME`/`CARGO_REGISTRIES_*` env var mutations (see
//! [`EnvGuard`]) - those are just as process-global as cwd, and real
//! `cargo metadata` shell-outs elsewhere in this binary depend on them.
//!
//! Each `tests/*.rs` top-level file is its own compiled test binary, so this
//! lock is per-binary, not shared across e.g. `unit.rs` and `cli_tests.rs` -
//! that's fine, since libtest already runs a single binary's tests in one
//! process while separate binaries run as separate processes.

#![allow(dead_code)]

use std::sync::{Mutex, OnceLock};

pub fn stable_cwd_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// RAII guard that restores an env var to its exact prior state when dropped.
///
/// Restores on drop - including on a mid-test panic, unlike a manual
/// `envmnt::remove(...)` at the end of a test function. Blindly removing a
/// var a test didn't itself set (rather than restoring whatever was there
/// before) silently assumes the pre-test state was "unset", which is true on
/// a bare dev machine but false in CI, where `CARGO_HOME` and
/// `CARGO_REGISTRIES_ENNOR_*` are legitimately set for the whole process -
/// removing them permanently breaks every `cargo metadata` shell-out later
/// in this same test binary, not just the one test that meant to touch them.
/// Goes through `envmnt` rather than `std::env::set_var` directly since this
/// crate forbids `unsafe_code`.
#[derive(Debug)]
pub struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    #[must_use]
    pub fn set(key: &'static str, value: &str) -> Self {
        let previous = envmnt::get_set(key, value);
        Self { key, previous }
    }

    #[must_use]
    pub fn unset(key: &'static str) -> Self {
        let previous = envmnt::get_remove(key);
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => envmnt::set(self.key, value),
            None => envmnt::remove(self.key),
        }
    }
}
