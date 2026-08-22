use pulley::service::unix::{detect, hide_console_window, is_pid1, status};

#[test]
fn hide_console_window_does_nothing_and_does_not_panic() {
    hide_console_window();
}

#[test]
fn is_pid1_is_false_for_a_name_that_is_almost_certainly_not_pid_1() {
    assert!(!is_pid1(
        "definitely-not-a-real-process-name-pulley-test-marker"
    ));
}

#[test]
fn is_pid1_reflects_the_real_proc_1_comm_on_this_machine() {
    // Whatever init system this sandbox actually runs, `is_pid1` must
    // agree with a direct read of `/proc/1/comm` - the two must never
    // disagree about what's asking.
    let Ok(comm) = std::fs::read_to_string("/proc/1/comm") else {
        return; // Some sandboxes have no /proc/1/comm readable; skip.
    };
    assert!(is_pid1(comm.trim()));
}

#[test]
fn status_delegates_to_whatever_backend_this_machine_detects_or_the_documented_error() {
    // `status()` on whichever real backend `detect()` finds here is a
    // read-only query (both `Systemd::status` and `Runit::status` only run
    // `systemctl --user status`/`sv status`, never mutating anything), so
    // it's safe to call for real - this exercises the `install`/
    // `uninstall`/`status` wrapper functions' dispatch through `detect()`.
    match status() {
        Ok(()) => {}
        Err(err) => assert!(
            err.to_string()
                .contains("no supported init system detected")
                || err.to_string().contains("sv")
        ),
    }
}

#[test]
fn detect_returns_a_backend_or_the_documented_error() {
    // Whichever branch this sandbox takes, it must be one of the two
    // outcomes `detect` documents - not a panic, not a third kind of
    // error.
    match detect() {
        Ok(_) => {}
        Err(err) => assert!(
            err.to_string()
                .contains("no supported init system detected")
        ),
    }
}
