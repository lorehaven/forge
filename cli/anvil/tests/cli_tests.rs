//! End-to-end tests of the `anvil` binary, run as a child process.
//!
//! `list` and `config check` run for real against this workspace. Everything that would build, publish
//! or push (`release`, `docker *`, `build`, `test`, ...) runs in a throwaway git workspace (see
//! `cli/flow_support.rs`) with `cargo` and `docker` replaced by stand-ins that only record their
//! arguments, so nothing is ever published. Kept apart from `unit`, which holds what runs in-process.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_test::cmd::Command;
use quench_test::predicate::prelude::*;

fn anvil() -> Command {
    Command::cargo_bin("anvil").expect("binary built")
}

#[quench_test::test]
fn list_names_runs_through_the_full_binary_and_prints_workspace_members() {
    anvil()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["list", "--format", "names"])
        .assert()
        .success()
        .stdout(predicate::str::contains("anvil"));
}

#[quench_test::test]
fn list_json_runs_through_the_full_binary() {
    anvil()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["list", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"packages\""));
}

#[quench_test::test]
fn list_with_an_unknown_format_fails_through_the_full_binary() {
    anvil()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["list", "--format", "bogus"])
        .assert()
        .failure();
}

#[quench_test::test]
fn help_flag_short_circuits_before_the_dispatch_match() {
    anvil()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("workspace build"));
}

// `machete --json`/`deny --json` are read-only static analysis, same as the
// non-json versions in `workspace_tests.rs` - safe to run for real. Neither
// asserts success: this workspace having something to flag is a legitimate
// non-zero exit, not a test failure. The point is that stdout is nothing but
// the banner-free, parseable report - not that the report is empty.
#[quench_test::test]
fn machete_json_runs_through_the_full_binary_with_a_banner_free_stdout() {
    let assert = anvil()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["machete", "--json"])
        .assert();
    let output = assert.get_output();
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("Anvil CLI"),
        "--json stdout must not carry anvil's own banner"
    );
    let report: quench_serde::Value =
        quench_serde::json::from_slice(&output.stdout).expect("stdout is a single JSON document");
    assert!(
        report
            .get("findings")
            .is_some_and(quench_serde::Value::is_array),
        "report has a `findings` array: {report}"
    );
}

#[quench_test::test]
fn deny_json_runs_through_the_full_binary_with_a_banner_free_stdout() {
    let assert = anvil()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["deny", "--json"])
        .assert();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Anvil CLI"),
        "--json stdout must not carry anvil's own banner"
    );
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let _: quench_serde::Value =
            quench_serde::json::from_str(line).expect("each stdout line is its own JSON document");
    }
}

#[quench_test::test]
fn config_check_accepts_the_estates_own_anvil_toml() {
    anvil()
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args(["config", "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[quench_test::test]
fn config_check_rejects_a_malformed_anvil_toml_and_lists_every_problem() {
    let dir = quench_core::tmp::TempDir::new().expect("tempdir");
    std::fs::write(
        dir.path().join(".anvil.toml"),
        "bogus_top_level = 1\n\n[docker.modules.core]\npackages = []\n",
    )
    .expect("write fixture config");

    anvil()
        .current_dir(dir.path())
        .args(["config", "check"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("bogus_top_level"))
        .stdout(predicate::str::contains("docker.modules.core"));
}

#[quench_test::test]
fn the_configuration_may_be_split_over_the_files_of_an_anvil_directory() {
    let dir = quench_core::tmp::TempDir::new().expect("tempdir");
    let parts = dir.path().join(".anvil");
    std::fs::create_dir_all(&parts).expect("the directory");
    std::fs::write(parts.join("release.toml"), "[release]\nregistry = \"r\"\n").expect("write");
    std::fs::write(
        parts.join("install.toml"),
        "[install]\npackages = [\"p\"]\n",
    )
    .expect("write");
    // The credentials live in the directory too, and are not configuration: their keys are not anvil's.
    std::fs::write(
        parts.join("secret.toml"),
        "gatehouse_url = \"https://g\"\napp_token = \"t\"\n",
    )
    .expect("write");

    anvil()
        .current_dir(dir.path())
        .args(["config", "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"))
        .stdout(predicate::str::contains(".anvil/install.toml"))
        .stdout(predicate::str::contains("secret.toml").not());
}

#[quench_test::test]
fn config_check_names_the_files_that_set_the_same_value() {
    let dir = quench_core::tmp::TempDir::new().expect("tempdir");
    let parts = dir.path().join(".anvil");
    std::fs::create_dir_all(&parts).expect("the directory");
    std::fs::write(
        dir.path().join(".anvil.toml"),
        "[release]\nregistry = \"one\"\n",
    )
    .expect("write");
    std::fs::write(
        parts.join("release.toml"),
        "[release]\nregistry = \"two\"\n",
    )
    .expect("write");

    anvil()
        .current_dir(dir.path())
        .args(["config", "check"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("release.registry"))
        .stdout(predicate::str::contains("release.toml"));
}

#[path = "cli/command_flow_tests.rs"]
mod command_flow_tests;
#[path = "cli/flow_support.rs"]
mod flow_support;
#[path = "cli/release_flow_tests.rs"]
mod release_flow_tests;

quench_test::main!();
