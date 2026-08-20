//! The `run` step: a shell command, run by a shell.
//!
//! Pipelines expect `run = "cargo build | tee log"` to mean what it says, so
//! this deliberately does *not* split the command itself - `sh` does. That is
//! the difference between `run` and the tool steps, and it is why `run` is
//! documented as the escape hatch.

/// The shell every conveyor runtime has. Not bash: the service image is alpine,
/// where `/bin/sh` is busybox ash and bash is not installed.
pub const SHELL: &str = "sh";

/// The argv to spawn a `run` step's command text under [`SHELL`].
pub fn argv(command: &str) -> Vec<String> {
    vec![
        SHELL.to_string(),
        "-c".to_string(),
        command.to_string(),
        // `sh -c` assigns the next argument to $0. Naming it explicitly keeps
        // an error message from busybox reading "sh: cargo: not found" with no
        // hint of which step produced it.
        "conveyor-step".to_string(),
    ]
}
