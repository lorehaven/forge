pub mod cli;
pub mod client;
pub mod commands;
pub mod config;

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

    /// `tokio::sync::Mutex`, not `std::sync::Mutex`: `client.rs`'s
    /// `Client::new` tests hold this across an `.await`, and clippy's
    /// `await_holding_lock` (correctly) objects to doing that with a std
    /// mutex. Plain sync `#[test]`s use `blocking_lock()`, which is fine
    /// outside of an async execution context.
    pub static ENV_LOCK: Mutex<()> = Mutex::const_new(());

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
}
