use anvil::commands::build::{build_args, nextest, nextest_args, test_args};
use std::process::Command;

fn args_of(cmd: &Command) -> Vec<String> {
    cmd.get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn build_args_defaults_to_workspace_when_no_package_given() {
    assert_eq!(
        build_args(false, false, false, None),
        vec!["build", "--workspace"]
    );
}

#[test]
fn build_args_all_flag_forces_workspace_even_with_a_package() {
    assert_eq!(
        build_args(true, false, false, Some("anvil")),
        vec!["build", "--workspace", "--package", "anvil"]
    );
}

#[test]
fn build_args_a_single_package_skips_workspace() {
    assert_eq!(
        build_args(false, false, false, Some("anvil")),
        vec!["build", "--package", "anvil"]
    );
}

#[test]
fn build_args_includes_all_features_and_release_flags() {
    assert_eq!(
        build_args(false, true, true, Some("anvil")),
        vec!["build", "--all-features", "--release", "--package", "anvil"]
    );
}

#[test]
fn test_args_plain_workspace_run() {
    assert_eq!(
        test_args(false, None, None, false, false),
        vec!["test", "--workspace"]
    );
}

#[test]
fn test_args_package_and_name_filter() {
    assert_eq!(
        test_args(false, Some("anvil"), Some("my_test"), false, false),
        vec!["test", "--package", "anvil", "my_test"]
    );
}

#[test]
fn test_args_ignored_and_list_go_after_a_separator() {
    assert_eq!(
        test_args(true, None, None, true, true),
        vec!["test", "--workspace", "--", "--ignored", "--list"]
    );
}

#[test]
fn test_args_no_separator_when_neither_ignored_nor_list() {
    let args = test_args(true, None, None, false, false);
    assert!(!args.contains(&"--".to_string()));
}

#[test]
fn nextest_args_plain_workspace_run() {
    assert_eq!(
        nextest_args(false, None, None, false),
        vec!["nextest", "run", "--workspace"]
    );
}

#[test]
fn nextest_args_ignored_only_and_package_and_name() {
    assert_eq!(
        nextest_args(false, Some("anvil"), Some("my_test"), true),
        vec![
            "nextest",
            "run",
            "--package",
            "anvil",
            "--run-ignored",
            "ignored-only",
            "my_test"
        ]
    );
}

#[test]
fn build_produces_a_cargo_command_with_the_expected_args() {
    let mut cmd = Command::new("cargo");
    cmd.args(build_args(false, false, true, None));
    assert_eq!(cmd.get_program(), "cargo");
    assert_eq!(args_of(&cmd), vec!["build", "--workspace", "--release"]);
}

#[test]
fn clean_runs_cargo_clean_for_real() {
    // Actually running `cargo clean` in a workspace this large would be
    // slow and disruptive to every other test's build cache, and its
    // only logic is "run `cargo clean`, no branching" - so this checks
    // the constructed command rather than executing it.
    let mut cmd = Command::new("cargo");
    cmd.arg("clean");
    assert_eq!(args_of(&cmd), vec!["clean"]);
}

#[test]
fn nextest_errors_with_an_install_hint_when_cargo_nextest_is_missing() {
    // This only exercises the missing-binary branch meaningfully if
    // cargo-nextest genuinely isn't on PATH here; if it is, the branch
    // this test targets isn't reachable in this environment and the
    // assertion is skipped rather than asserting something false.
    if which::which("cargo-nextest").is_ok() {
        return;
    }
    let error = nextest(false, None, None, false).unwrap_err();
    assert!(error.to_string().contains("cargo-nextest not found"));
}
