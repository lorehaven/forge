use anyhow::{Context, Result};
use std::process::Command;

use crate::util::{run_command, run_command_json};

#[must_use]
pub fn build_args(
    all: bool,
    all_features: bool,
    release: bool,
    package: Option<&str>,
) -> Vec<String> {
    let mut args = vec!["build".to_string()];

    if all || package.is_none() {
        args.push("--workspace".to_string());
    }

    if all_features {
        args.push("--all-features".to_string());
    }

    if release {
        args.push("--release".to_string());
    }

    if let Some(pkg) = package {
        args.push("--package".to_string());
        args.push(pkg.to_string());
    }

    args
}

// `package`/`test_name` arrive here as owned `Option<String>` straight out of
// clap's parsed args (see `main.rs`'s dispatch) - that's the natural shape of
// this CLI entry point's own API, even though the body below only borrows
// them via `.as_deref()`.
#[allow(clippy::needless_pass_by_value)]
pub fn build(all: bool, all_features: bool, release: bool, package: Option<String>) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args(build_args(all, all_features, release, package.as_deref()));

    run_command(cmd, "build")
}

pub fn clean() -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("clean");

    run_command(cmd, "clean")
}

#[must_use]
pub fn test_args(
    all: bool,
    package: Option<&str>,
    test_name: Option<&str>,
    ignored: bool,
    list: bool,
    json: bool,
) -> Vec<String> {
    let mut args = Vec::new();

    // The stable libtest harness has no structured-output format of its own;
    // `-Z unstable-options --format json` is nightly-only.
    if json {
        args.push("+nightly".to_string());
    }
    args.push("test".to_string());

    if all || package.is_none() {
        args.push("--workspace".to_string());
    }

    if let Some(pkg) = package {
        args.push("--package".to_string());
        args.push(pkg.to_string());
    }

    if let Some(name) = test_name {
        args.push(name.to_string());
    }

    let mut trailing = Vec::new();
    if ignored {
        trailing.push("--ignored".to_string());
    }
    if list {
        trailing.push("--list".to_string());
    }
    if json {
        trailing.push("-Z".to_string());
        trailing.push("unstable-options".to_string());
        trailing.push("--format".to_string());
        trailing.push("json".to_string());
    }
    if !trailing.is_empty() {
        args.push("--".to_string());
        args.extend(trailing);
    }

    args
}

#[allow(clippy::needless_pass_by_value)]
pub fn test(
    all: bool,
    package: Option<String>,
    test_name: Option<String>,
    ignored: bool,
    list: bool,
    json: bool,
) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args(test_args(
        all,
        package.as_deref(),
        test_name.as_deref(),
        ignored,
        list,
        json,
    ));

    if json {
        return run_command_json(cmd);
    }

    run_command(cmd, "test")
}

/// Same test selection as `test`, run through cargo-nextest instead of the
/// built-in libtest harness.
///
/// Parallel-by-default, per-test timeouts, output that doesn't interleave
/// across crates. Doesn't cover doctests (nextest doesn't run them) or
/// `forge-bdd`'s cucumber suite (that's a binary target driven by
/// `foreman test`, not a `#[test]`-based harness).
#[must_use]
pub fn nextest_args(
    all: bool,
    package: Option<&str>,
    test_name: Option<&str>,
    ignored: bool,
    json: bool,
) -> Vec<String> {
    let mut args = vec!["nextest".to_string(), "run".to_string()];

    // nextest's own experimental structured-output format; enabling it also
    // requires the `NEXTEST_EXPERIMENTAL_LIBTEST_JSON` env var (see `nextest`
    // below) since it isn't stabilized yet.
    if json {
        args.push("--message-format".to_string());
        args.push("libtest-json-plus".to_string());
    }

    if all || package.is_none() {
        args.push("--workspace".to_string());
    }

    if let Some(pkg) = package {
        args.push("--package".to_string());
        args.push(pkg.to_string());
    }

    if ignored {
        args.push("--run-ignored".to_string());
        args.push("ignored-only".to_string());
    }

    if let Some(name) = test_name {
        args.push(name.to_string());
    }

    args
}

#[allow(clippy::needless_pass_by_value)]
pub fn nextest(
    all: bool,
    package: Option<String>,
    test_name: Option<String>,
    ignored: bool,
    json: bool,
) -> Result<()> {
    which::which("cargo-nextest")
        .context("cargo-nextest not found. Install with: cargo install cargo-nextest")?;

    let mut cmd = Command::new("cargo");
    cmd.args(nextest_args(
        all,
        package.as_deref(),
        test_name.as_deref(),
        ignored,
        json,
    ));

    if json {
        cmd.env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1");
        return run_command_json(cmd);
    }

    run_command(cmd, "nextest")
}
