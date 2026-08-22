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
