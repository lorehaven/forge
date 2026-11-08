pub mod cli;
pub mod client;
pub mod commands;
pub mod config;

/// The env var lock and guard of this crate's tests.
///
/// Shared by every test in this crate's `tests/unit.rs` binary that touches
/// an env var `Client::new`/`FileConfig::load` read - `config.rs`'s own
/// tests and `client.rs`'s `Client::new` tests both need it, and nextest
/// runs every test as its own process while cargo's default `--lib` runner
/// runs them in parallel threads within one - either way, an env var lock
/// must live in one place shared by both files, not be reinvented per file.
///
/// Crucially, some environments have a REAL `~/.config/conveyor/config.toml`
/// with live admin credentials pointing at a real server - `Client::new`'s
/// tests must override `XDG_CONFIG_HOME` to a directory with no such file,
/// or they would log in for real. `EnvGuard` restores whatever was there
/// before on drop either way, so a real `XDG_CONFIG_HOME`/`HOME`, if ever
/// set, comes back after.
///
/// Not `#[cfg(test)]`: that flag is only set when *this* crate is compiled
/// for its own tests, not when the separate `tests/` integration binary
/// links this crate as an ordinary dependency - so a `#[cfg(test)]`-gated
/// module would not exist in the build the `tests/` crate sees at all.
#[allow(dead_code)]
pub mod test_support {
    use tokio::sync::Mutex;

    /// The lock every test that touches an env var holds.
    ///
    /// `tokio::sync::Mutex`, not `std::sync::Mutex`: `client.rs`'s
    /// `Client::new` tests hold this across an `.await`, and clippy's
    /// `await_holding_lock` (correctly) objects to doing that with a std
    /// mutex. Plain sync `#[test]`s use `blocking_lock()`, which is fine
    /// outside of an async execution context.
    pub static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    /// Sets or removes an env var, and puts back what was there before on drop.
    ///
    /// Goes through `quench_core::env`, since this crate forbids `unsafe_code`.
    #[derive(Debug)]
    pub struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        #[must_use]
        pub fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            // Serialized by ENV_LOCK, held for the guard's lifetime.
            quench_core::env::set(key, value);
            Self { key, previous }
        }

        #[must_use]
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
}
