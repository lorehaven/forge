use crate::env_support::{ENV_LOCK, EnvGuard};
use pulley::service::unix::Backend;
use pulley::service::unix::systemd::{
    Systemd, linger_enabled, run_systemctl, unit_file_contents, user_unit_dir, whoami,
};
use std::path::PathBuf;

// This machine has a real `systemctl` on PATH (it's a real systemd-based
// desktop, not a container), so `Backend::install`/`uninstall` for `Systemd`
// are never called here - they enable/write/reload a real user systemd
// unit, which would mutate this actual machine's session. `status` and
// `run_systemctl`/`linger_enabled` below are read-only queries (`systemctl
// --user status`/`is-active`, `loginctl show-user ... --property=Linger`),
// safe to run for real: they only report on `pulley.service`'s (almost
// certainly nonexistent) state, never change anything.

#[test]
fn status_queries_systemctl_without_erroring_even_when_the_unit_does_not_exist() {
    Systemd.status().expect("status");
}

#[test]
fn run_systemctl_errors_for_a_unit_that_does_not_exist() {
    let err = run_systemctl(&[
        "is-active",
        "definitely-not-a-real-pulley-test-unit.service",
    ])
    .unwrap_err();
    assert!(err.to_string().contains("systemctl"));
}

#[test]
fn linger_enabled_returns_a_bool_without_erroring() {
    // Whatever this machine's actual linger state is, the call itself must
    // succeed and parse to a bool - not panic or error just because linger
    // happens to be off.
    linger_enabled().expect("linger_enabled");
}

#[test]
fn unit_file_contents_embeds_the_executable_path_and_the_daemon_subcommand() {
    let contents = unit_file_contents(std::path::Path::new("/usr/local/bin/pulley"));
    assert!(contents.contains("ExecStart=/usr/local/bin/pulley daemon"));
    assert!(contents.contains("[Unit]"));
    assert!(contents.contains("[Service]"));
    assert!(contents.contains("[Install]"));
    assert!(contents.contains("Restart=on-failure"));
}

#[test]
fn whoami_returns_the_user_env_var() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _guard = EnvGuard::set("USER", "alice");
    assert_eq!(whoami(), "alice");
}

#[test]
fn whoami_falls_back_to_a_placeholder_when_unset() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _guard = EnvGuard::unset("USER");
    assert_eq!(whoami(), "$USER");
}

#[test]
fn user_unit_dir_is_under_home_config_systemd_user() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _guard = EnvGuard::set("HOME", "/home/alice");
    assert_eq!(
        user_unit_dir().unwrap(),
        PathBuf::from("/home/alice/.config/systemd/user")
    );
}

#[test]
fn user_unit_dir_errors_without_home() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _guard = EnvGuard::unset("HOME");
    assert!(user_unit_dir().is_err());
}
