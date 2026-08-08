#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{install, status, uninstall};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{install, status, uninstall};
