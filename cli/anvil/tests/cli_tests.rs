//! End-to-end coverage of `main.rs`'s dispatch, which has no other way to be
//! exercised - `Commands::Build`/`Test`/`Release`/`Docker::*` all shell out
//! to real cargo/git/docker operations that would be slow or actively
//! harmful to run in a test (`anvil release` creates commits and tags;
//! `anvil docker build` needs a real Dockerfile and daemon), so only
//! `list` - read-only, backed by a real `cargo metadata` call - is driven
//! through the compiled binary here. Everything else in `main.rs`'s dispatch
//! table is a one-line match arm identical in shape to this one.

use assert_cmd::Command;
use predicates::prelude::*;

fn anvil() -> Command {
    Command::cargo_bin("anvil").expect("binary built")
}

#[test]
fn list_names_runs_through_the_full_binary_and_prints_workspace_members() {
    anvil()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["list", "--format", "names"])
        .assert()
        .success()
        .stdout(predicate::str::contains("anvil"));
}

#[test]
fn list_json_runs_through_the_full_binary() {
    anvil()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["list", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"packages\""));
}

#[test]
fn list_with_an_unknown_format_fails_through_the_full_binary() {
    anvil()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["list", "--format", "bogus"])
        .assert()
        .failure();
}

#[test]
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
#[test]
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
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is a single JSON document");
    assert!(
        report
            .get("findings")
            .is_some_and(serde_json::Value::is_array),
        "report has a `findings` array: {report}"
    );
}

#[test]
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
        let _: serde_json::Value =
            serde_json::from_str(line).expect("each stdout line is its own JSON document");
    }
}

#[test]
fn config_check_accepts_the_estates_own_anvil_toml() {
    anvil()
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args(["config", "check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[test]
fn config_check_rejects_a_malformed_anvil_toml_and_lists_every_problem() {
    let dir = tempfile::tempdir().expect("tempdir");
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
