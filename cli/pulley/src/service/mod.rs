use std::path::Path;

#[cfg(unix)]
pub mod unix;
#[cfg(unix)]
pub use unix::{install, status, uninstall};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{install, status, uninstall};

/// What the Windows scheduled task runs: the daemon of the pulley at `exe`, under a console host without a window.
///
/// Started by itself, a console program opens a window at every logon.
#[must_use]
pub fn task_command(exe: &Path) -> String {
    format!("conhost.exe --headless \"{}\" daemon", exe.display())
}
