//! End-to-end coverage of foundry's command dispatch (`main.rs`), which has
//! no `[lib]` target to unit-test directly - only `assert_cmd` against the
//! built binary reaches it.
//!
//! `apply`/`status` need a real Postgres with the catalog already installed;
//! set `FOUNDRY_TEST_DATABASE_URL` to run them (see
//! `docker/workbench-service/tests/integration/support.rs` for how to stand
//! one up). They skip - not fail - without it, matching that convention.

use assert_cmd::Command;
use predicates::prelude::*;

fn foundry() -> Command {
    Command::cargo_bin("foundry-service").expect("binary built")
}

fn test_database_url() -> Option<String> {
    std::env::var("FOUNDRY_TEST_DATABASE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
}

#[test]
fn validate_accepts_the_real_catalog_and_config() {
    // `validate` never needs a database, so this runs unconditionally.
    foundry()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("catalog and plan are consistent"));
}

#[test]
fn validate_rejects_an_unknown_module() {
    foundry()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--install", "not-a-real-module", "validate"])
        .assert()
        .failure();
}

#[test]
fn apply_without_a_database_url_refuses_to_start() {
    let dir = tempfile::tempdir().expect("tempdir");
    foundry()
        .current_dir(&dir)
        .env_remove("DATABASE_URL")
        .env_remove("POSTGRES_URL")
        .env_remove("FOUNDRY_CONFIG")
        .args([
            "--catalog",
            &format!("{}/migrations", env!("CARGO_MANIFEST_DIR")),
            "--install",
            "gatehouse",
            "apply",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no database configured"));
}

#[test]
fn reset_without_yes_refuses_and_lists_the_schemas_it_would_drop() {
    let Some(url) = test_database_url() else {
        eprintln!(
            "skipping reset_without_yes_refuses_and_lists_the_schemas_it_would_drop: FOUNDRY_TEST_DATABASE_URL is not set"
        );
        return;
    };

    foundry()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--database-url", &url, "--install", "gatehouse", "reset"])
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "refusing to drop schemas without --yes",
        ));
}

#[test]
fn apply_against_the_test_database_reports_up_to_date_modules() {
    let Some(url) = test_database_url() else {
        eprintln!(
            "skipping apply_against_the_test_database_reports_up_to_date_modules: FOUNDRY_TEST_DATABASE_URL is not set"
        );
        return;
    };

    // The shared test database already has every catalog module installed
    // (see the coverage-push setup), so this exercises the "nothing to do"
    // path rather than actually writing migrations.
    foundry()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--database-url", &url, "apply"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already up to date"));
}

#[test]
fn status_against_the_test_database_lists_installed_modules() {
    let Some(url) = test_database_url() else {
        eprintln!(
            "skipping status_against_the_test_database_lists_installed_modules: FOUNDRY_TEST_DATABASE_URL is not set"
        );
        return;
    };

    foundry()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--database-url", &url, "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed"));
}

#[test]
fn plan_against_the_test_database_is_a_dry_run() {
    let Some(url) = test_database_url() else {
        eprintln!(
            "skipping plan_against_the_test_database_is_a_dry_run: FOUNDRY_TEST_DATABASE_URL is not set"
        );
        return;
    };

    foundry()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--database-url", &url, "plan"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry-run"));
}
