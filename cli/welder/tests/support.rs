//! Shared across every test file in this crate's `unit` test binary (they
//! all run in one process, in parallel by default). Any test that calls
//! `std::env::set_current_dir` (process-global) must hold `cwd_lock()` for
//! as long as the cwd is changed - and, just as importantly, any test that
//! forces `welder::config::CONFIG` (a `LazyLock` that reads
//! `cwd/.welder/config.toml` on its *first* touch from anywhere in the
//! process and then keeps that value forever) must also hold it while it
//! does, so it can't be forced mid-flight by another test that's temporarily
//! pointed the cwd at a directory with no config file.
#![allow(dead_code)]

use std::sync::{Mutex, OnceLock};

pub fn cwd_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| {
        // Every test that changes the cwd takes this lock first, so this runs before the cwd has ever been
        // changed: `CONFIG` is read from the crate directory here, whichever test (locked or not) would have
        // forced it first - one that did it while another had the cwd in a temp dir got the defaults.
        std::sync::LazyLock::force(&welder::config::CONFIG);
        Mutex::new(())
    })
}
