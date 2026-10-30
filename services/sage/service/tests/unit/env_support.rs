//! One shared lock for every test in this `tests/unit.rs` binary that reads
//! or writes a fixed-name env var (search-provider API keys, `SAGE_*`
//! config vars, `DB_SCHEMA`, `SKIP_SWITCHBOARD_CHECK`, `FILE_OPS_BASE_PATH`,
//! ...). All `tests/unit/*.rs` files compile into this one binary and run
//! in parallel by default, so a per-file lock does nothing to stop a test in
//! one file racing a test in another that touches the same var. Coarse but
//! simple: one lock for every env-touching test here, rather than one per
//! variable name.

use std::sync::{Mutex, OnceLock};

pub fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
