#[cfg(unix)]
pub mod unix;
#[cfg(unix)]
pub use unix::{hide_console_window, install, status, uninstall};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{hide_console_window, install, status, uninstall};
