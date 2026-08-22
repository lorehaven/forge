//! Shared across every test binary in this crate's `tests/` suite that needs
//! `std::process::Command`'s cwd-relative resolution to be stable. Real
//! `cargo metadata`/`git log` calls with no explicit `--manifest-path`, or
//! `std::env::current_dir()` reads that expect to see this crate's directory,
//! must hold `stable_cwd_lock()` for the duration it cares about, and any
//! test that actually calls `std::env::set_current_dir` (cwd is
//! process-global) must hold it for as long as the cwd is changed.
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
