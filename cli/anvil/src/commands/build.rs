use anyhow::{Context, Result};
use std::process::Command;

use crate::util::run_command;

pub fn build(all: bool, all_features: bool, release: bool, package: Option<String>) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build");

    if all || package.is_none() {
        cmd.arg("--workspace");
    }

    if all_features {
        cmd.arg("--all-features");
    }

    if release {
        cmd.arg("--release");
    }

    if let Some(pkg) = package {
        cmd.arg("--package").arg(pkg);
    }

    run_command(cmd, "build")
}

pub fn clean() -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("clean");

    run_command(cmd, "clean")
}

pub fn test(
    all: bool,
    package: Option<String>,
    test_name: Option<String>,
    ignored: bool,
    list: bool,
) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("test");

    if all || package.is_none() {
        cmd.arg("--workspace");
    }

    if let Some(pkg) = package {
        cmd.arg("--package").arg(pkg);
    }

    if let Some(name) = test_name {
        cmd.arg(name);
    }

    if ignored || list {
        cmd.arg("--");
        if ignored {
            cmd.arg("--ignored");
        }
        if list {
            cmd.arg("--list");
        }
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
pub fn nextest(
    all: bool,
    package: Option<String>,
    test_name: Option<String>,
    ignored: bool,
) -> Result<()> {
    which::which("cargo-nextest")
        .context("cargo-nextest not found. Install with: cargo install cargo-nextest")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("nextest").arg("run");

    if all || package.is_none() {
        cmd.arg("--workspace");
    }

    if let Some(pkg) = package {
        cmd.arg("--package").arg(pkg);
    }

    if ignored {
        cmd.arg("--run-ignored").arg("ignored-only");
    }

    if let Some(name) = test_name {
        cmd.arg(name);
    }

    run_command(cmd, "nextest")
}
