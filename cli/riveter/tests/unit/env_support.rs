//! Shared lock for tests across this binary that rely on `std::env::current_dir`
//! resolving to a directory they control (`config.rs`/`env.rs` both read
//! fixed, cwd-relative paths - `.riveter.toml`, `overlays/`, `manifests/`).
//! `std::env::set_current_dir` is process-global, and cargo runs every test
//! in this binary in parallel by default, so any test that changes cwd must
//! hold this lock for as long as the cwd matters to it.

use std::sync::{Mutex, OnceLock};

pub fn cwd_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
