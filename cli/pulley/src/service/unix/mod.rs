//! Dispatches `pulley service` to whichever service manager actually runs
//! this system, detected at call time. Adding another one (OpenRC, s6, ...)
//! means adding a submodule that implements `Backend` and a branch in
//! `detect()` — nothing else in the crate needs to change.

mod runit;
mod systemd;

use std::path::Path;

trait Backend {
    fn install(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn uninstall(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn status(&self) -> Result<(), Box<dyn std::error::Error>>;
}

/// No-op here: journald/runit's own log chain already captures a service's
/// output, so there's no console window to hide.
pub fn hide_console_window() {}

pub fn install() -> Result<(), Box<dyn std::error::Error>> {
    detect()?.install()
}

pub fn uninstall() -> Result<(), Box<dyn std::error::Error>> {
    detect()?.uninstall()
}

pub fn status() -> Result<(), Box<dyn std::error::Error>> {
    detect()?.status()
}

fn detect() -> Result<Box<dyn Backend>, Box<dyn std::error::Error>> {
    // Canonical systemd-is-running check (documented by systemd itself),
    // rather than just probing for `systemctl` on PATH, which can exist
    // without systemd actually being PID 1 (e.g. some containers).
    if Path::new("/run/systemd/system").exists() {
        return Ok(Box::new(systemd::Systemd));
    }
    if is_pid1("runit") {
        return Ok(Box::new(runit::Runit));
    }
    Err(
        "no supported init system detected (looked for systemd at /run/systemd/system, \
         runit as PID 1); pulley service management currently supports systemd and runit"
            .into(),
    )
}

fn is_pid1(name: &str) -> bool {
    std::fs::read_to_string("/proc/1/comm")
        .map(|comm| comm.trim() == name)
        .unwrap_or(false)
}
