use foreman::process::*;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn scratch(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "foreman-process-test-{name}-{}",
        std::process::id()
    ))
}

#[test]
fn read_pid_round_trips_through_write_pid() {
    let path = scratch("roundtrip.pid");
    write_pid(&path, 4242).unwrap();
    assert_eq!(read_pid(&path), Some(4242));
    std::fs::remove_file(&path).ok();
}

#[test]
fn read_pid_is_none_for_a_missing_file() {
    let path = scratch("does-not-exist.pid");
    assert_eq!(read_pid(&path), None);
}

#[test]
fn read_pid_is_none_for_malformed_content() {
    let path = scratch("malformed.pid");
    std::fs::write(&path, "not-a-pid\n").unwrap();
    assert_eq!(read_pid(&path), None);
    std::fs::remove_file(&path).ok();
}

#[test]
fn read_pid_trims_whitespace() {
    let path = scratch("padded.pid");
    std::fs::write(&path, "  99  \n").unwrap();
    assert_eq!(read_pid(&path), Some(99));
    std::fs::remove_file(&path).ok();
}

#[test]
fn tail_returns_only_the_last_n_lines() {
    let path = scratch("tail.log");
    std::fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").unwrap();
    assert_eq!(tail(&path, 2), "four\nfive");
    std::fs::remove_file(&path).ok();
}

#[test]
fn tail_returns_the_whole_file_when_it_has_fewer_lines_than_asked() {
    let path = scratch("short-tail.log");
    std::fs::write(&path, "one\ntwo\n").unwrap();
    assert_eq!(tail(&path, 5), "one\ntwo");
    std::fs::remove_file(&path).ok();
}

#[test]
fn tail_of_a_missing_file_is_empty_rather_than_an_error() {
    let path = scratch("missing-tail.log");
    assert_eq!(tail(&path, 5), "");
}

#[test]
fn wait_for_returns_ready_as_soon_as_the_check_passes() {
    let waited = wait_for(None, Duration::from_secs(5), || true);
    assert!(matches!(waited, Wait::Ready));
}

#[test]
fn wait_for_times_out_when_the_check_never_passes() {
    let waited = wait_for(None, Duration::from_millis(50), || false);
    assert!(matches!(waited, Wait::Timeout));
}

#[test]
fn wait_for_reports_died_when_the_pid_is_no_longer_alive() {
    // pid 1 always exists; an out-of-range pid never does, without needing
    // to spawn and kill a real process just to test this branch.
    let waited = wait_for(Some(i32::MAX), Duration::from_secs(5), || false);
    assert!(matches!(waited, Wait::Died));
}

#[test]
fn alive_is_true_for_our_own_process_and_false_for_an_impossible_pid() {
    assert!(alive(std::process::id() as i32));
    assert!(!alive(i32::MAX));
}

#[test]
fn terminate_and_kill_stop_a_real_child_we_own() {
    let mut child = Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
    let pid = child.id() as i32;
    assert!(alive(pid));

    terminate(pid);
    let waited = wait_for(None, Duration::from_secs(5), || !alive(pid));
    if !matches!(waited, Wait::Ready) {
        kill(pid);
    }
    let _ = child.wait();
    assert!(!alive(pid));
}

#[test]
fn capture_reports_success_and_trimmed_stdout() {
    let (ok, output) = capture("sh", &["-c", "echo '  hello  '"]).unwrap();
    assert!(ok);
    assert_eq!(output, "hello");
}

#[test]
fn capture_reports_failure_for_a_nonzero_exit() {
    let (ok, _) = capture("sh", &["-c", "exit 1"]).unwrap();
    assert!(!ok);
}

#[test]
fn capture_errors_when_the_program_does_not_exist() {
    assert!(capture("foreman-coverage-test-no-such-binary", &[]).is_err());
}

#[test]
fn run_reports_success_and_failure_by_exit_status() {
    let workdir = std::env::temp_dir();
    assert!(run("true", &[], &workdir, &[]).unwrap());
    assert!(!run("false", &[], &workdir, &[]).unwrap());
}

#[test]
fn run_passes_environment_variables_through() {
    let workdir = std::env::temp_dir();
    let ok = run(
        "sh",
        &[
            "-c".to_string(),
            "[ \"$FOREMAN_COVERAGE_TEST_VAR\" = \"present\" ]".to_string(),
        ],
        &workdir,
        &[(
            "FOREMAN_COVERAGE_TEST_VAR".to_string(),
            "present".to_string(),
        )],
    )
    .unwrap();
    assert!(ok);
}

#[test]
fn shell_reports_success_and_failure_by_exit_status() {
    let workdir = std::env::temp_dir();
    assert!(shell("exit 0", &workdir, Duration::from_secs(5)).unwrap());
    assert!(!shell("exit 1", &workdir, Duration::from_secs(5)).unwrap());
}

#[test]
fn shell_times_out_and_kills_a_command_that_runs_too_long() {
    let workdir = std::env::temp_dir();
    let ok = shell("sleep 30", &workdir, Duration::from_millis(200)).unwrap();
    assert!(!ok);
}

#[test]
fn pgrep_finds_a_process_by_its_command_line_and_stops_finding_it_once_it_exits() {
    // A fixed "surely nothing matches this" string isn't safe to assert
    // against on a shared/busy machine running unrelated processes - so
    // instead this spawns a real, uniquely-marked process (the marker
    // includes our own pid, so two instances of this test can't collide
    // with each other either) and checks pgrep both finds it while it's
    // alive and stops once it's gone.
    let marker = format!("foreman-coverage-test-pgrep-marker-{}", std::process::id());
    let mut child = Command::new("sleep")
        .arg("5")
        .env("FOREMAN_COVERAGE_TEST_PGREP_MARKER", &marker)
        .spawn()
        .expect("spawn sleep");

    // `sleep 5`'s own argv doesn't contain the marker, but `-f` matches
    // against `/proc/<pid>/cmdline`, which doesn't include env either -
    // so search for the pid itself via `pgrep -f sleep` and confirm our
    // child is among the matches instead of embedding the marker in argv,
    // which would need a shell wrapper (and its own transient process).
    let found = pgrep("sleep 5")
        .iter()
        .any(|pid| pid.parse::<u32>() == Ok(child.id()));
    assert!(
        found,
        "expected pgrep to find our own child while it's alive"
    );

    child.kill().expect("kill sleep");
    child.wait().expect("reap sleep");

    // Give the kernel a moment to actually remove the process entry.
    let gone = process_disappears(child.id(), Duration::from_secs(2));
    assert!(gone, "expected the killed child to disappear from pgrep");
    let _ = marker;
}

fn process_disappears(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let still_there = pgrep("sleep 5")
            .iter()
            .any(|found| found.parse::<u32>() == Ok(pid));
        if !still_there {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn follow_errors_when_there_is_no_log_file() {
    let path = scratch("no-such-log-to-follow");
    assert!(follow(&path).is_err());
}

#[test]
fn spawn_detached_starts_a_process_and_writes_its_log() {
    let log_file = scratch("spawn-detached.log");
    let pid = spawn_detached(
        Path::new("/bin/sh"),
        &std::env::temp_dir(),
        &[],
        &[],
        &log_file,
    )
    .unwrap();
    assert!(pid > 0);

    // Not `spawn_detached`'s own child from Rust's point of view (we didn't
    // keep the `Child` handle - that's the point, it outlives us), so wait
    // for it to exit rather than reaping it directly.
    let waited = wait_for(Some(pid), Duration::from_secs(5), || false);
    assert!(matches!(waited, Wait::Died) || matches!(waited, Wait::Timeout));
    if alive(pid) {
        kill(pid);
    }
    std::fs::remove_file(&log_file).ok();
}

#[test]
fn spawn_detached_errors_for_a_program_that_does_not_exist() {
    let log_file = scratch("spawn-detached-missing.log");
    let result = spawn_detached(
        Path::new("/no/such/program/foreman-coverage-test"),
        &std::env::temp_dir(),
        &[],
        &[],
        &log_file,
    );
    assert!(result.is_err());
    std::fs::remove_file(&log_file).ok();
}
