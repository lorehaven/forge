use anvil::util::{log_file_path, print_log_tail, run_command};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn log_file_path_slugifies_non_alphanumeric_characters() {
    let path = log_file_path("Docker Build (release)!");
    assert_eq!(
        path,
        PathBuf::from("target/anvil-logs/docker-build--release.log")
    );
}

#[test]
fn log_file_path_trims_leading_and_trailing_dashes() {
    let path = log_file_path("--release--");
    assert_eq!(path, PathBuf::from("target/anvil-logs/release.log"));
}

#[test]
fn log_file_path_falls_back_to_operation_when_slug_is_empty() {
    let path = log_file_path("---");
    assert_eq!(path, PathBuf::from("target/anvil-logs/operation.log"));
}

#[test]
fn run_command_succeeds_and_writes_a_log_file() {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "echo hello-from-anvil-test"]);
    run_command(cmd, "util test success case").expect("command succeeds");

    let log_path = log_file_path("util test success case");
    let contents = fs::read_to_string(&log_path).expect("log file exists");
    assert!(contents.contains("hello-from-anvil-test"));
}

#[test]
fn run_command_reports_an_error_for_a_failing_status_and_still_logs() {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "echo boom >&2; exit 7"]);
    let error = run_command(cmd, "util test failure case").unwrap_err();

    assert!(error.to_string().contains("failed with status"));
    let log_path = log_file_path("util test failure case");
    let contents = fs::read_to_string(&log_path).expect("log file exists");
    assert!(contents.contains("boom"));
}

#[test]
fn run_command_errors_when_the_program_does_not_exist() {
    let cmd = Command::new("definitely-not-a-real-binary-anvil-test");
    let error = run_command(cmd, "util test missing binary").unwrap_err();
    assert!(error.to_string().contains("Failed to execute"));
}

#[test]
fn print_log_tail_handles_a_missing_log_file_without_panicking() {
    print_log_tail(&PathBuf::from(
        "target/anvil-logs/does-not-exist-anvil-test.log",
    ));
}

#[test]
fn print_log_tail_handles_an_empty_log_file_without_panicking() {
    let dir = std::env::temp_dir().join("anvil-util-test-empty-log");
    fs::create_dir_all(&dir).expect("create dir");
    let path = dir.join("empty.log");
    fs::write(&path, "").expect("write empty log");

    print_log_tail(&path);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn print_log_tail_prints_only_the_last_lines_of_a_long_log() {
    let dir = std::env::temp_dir().join("anvil-util-test-long-log");
    fs::create_dir_all(&dir).expect("create dir");
    let path = dir.join("long.log");
    let body = (0..200)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, body).expect("write long log");

    // No direct way to capture eprintln! output here; this just proves
    // the function runs to completion (no panic/overflow) on a log
    // longer than FAILURE_TAIL_LINES, which is the branch that used to
    // risk a `saturating_sub` edge case.
    print_log_tail(&path);

    let _ = fs::remove_dir_all(&dir);
}
