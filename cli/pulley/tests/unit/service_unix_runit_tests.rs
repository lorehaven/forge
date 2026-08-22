use crate::env_support::{ENV_LOCK, EnvGuard};
use pulley::service::unix::Backend;
use pulley::service::unix::runit::{
    Runit, require_root, run_script_contents, scan_dir, set_executable, target_user,
};
use std::path::Path;
use std::process::Command;

// `sv`/`chpst` are not installed in this environment (it's runit-specific
// tooling; this machine is not running runit), so every `Backend` method
// below deterministically fails at its first `require_binary` check -
// before touching any real service file, symlink, or `sv`/`systemctl`
// process. That's a safe, real assertion about the error path, not a
// no-op: it would break if `install`/`uninstall`/`status` ever stopped
// checking for the binary before doing anything else.

#[test]
fn install_fails_fast_without_sv_installed() {
    let err = Runit.install().unwrap_err();
    assert!(err.to_string().contains("sv") || err.to_string().contains("runit"));
}

#[test]
fn uninstall_fails_fast_without_sv_installed() {
    let err = Runit.uninstall().unwrap_err();
    assert!(err.to_string().contains("sv") || err.to_string().contains("runit"));
}

#[test]
fn status_fails_fast_without_sv_installed() {
    let err = Runit.status().unwrap_err();
    assert!(err.to_string().contains("sv") || err.to_string().contains("runit"));
}

const SCAN_DIRS: &[&str] = &["/var/service", "/etc/service", "/run/runit/service"];

#[test]
fn run_script_contents_drops_privileges_to_the_target_user_and_runs_daemon() {
    let script = run_script_contents(Path::new("/usr/local/bin/pulley"), "alice");
    assert!(script.starts_with("#!/bin/sh\n"));
    assert!(script.contains("exec chpst -u alice /usr/local/bin/pulley daemon"));
}

#[test]
fn target_user_prefers_sudo_user_over_user() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _sudo = EnvGuard::set("SUDO_USER", "alice");
    let _user = EnvGuard::set("USER", "root");
    assert_eq!(target_user(), "alice");
}

#[test]
fn target_user_falls_back_to_user_without_sudo_user() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _sudo = EnvGuard::unset("SUDO_USER");
    let _user = EnvGuard::set("USER", "bob");
    assert_eq!(target_user(), "bob");
}

#[test]
fn target_user_falls_back_to_root_when_neither_is_set() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _sudo = EnvGuard::unset("SUDO_USER");
    let _user = EnvGuard::unset("USER");
    assert_eq!(target_user(), "root");
}

#[test]
fn scan_dir_returns_one_of_the_documented_candidates_or_none() {
    // Real filesystem check on whatever this sandbox actually has -
    // must land on one of the three fixed candidates, or None, never
    // panic or return something else.
    match scan_dir() {
        None => {}
        Some(dir) => assert!(
            SCAN_DIRS
                .iter()
                .any(|candidate| Path::new(candidate) == dir),
            "{dir:?} was not one of {SCAN_DIRS:?}"
        ),
    }
}

#[test]
fn set_executable_sets_the_owner_execute_bit() {
    use std::os::unix::fs::PermissionsExt;

    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::set_permissions(file.path(), std::fs::Permissions::from_mode(0o644)).unwrap();

    set_executable(file.path()).unwrap();

    let mode = std::fs::metadata(file.path()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
}

#[test]
fn require_root_agrees_with_a_direct_id_check() {
    // Harmless real `id -u` call (read-only) - not a real "install",
    // just proving the messaging/branching around it is right for
    // whatever this sandbox's actual uid is.
    let output = Command::new("id").arg("-u").output().unwrap();
    let is_root = String::from_utf8_lossy(&output.stdout).trim() == "0";

    let result = require_root("install");
    assert_eq!(result.is_ok(), is_root);
    if let Err(err) = result {
        assert!(err.to_string().contains("sudo"));
    }
}
