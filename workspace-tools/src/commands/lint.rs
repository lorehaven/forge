use anyhow::Result;
use std::process::Command;

use crate::util::run_command;

pub fn lint(all_targets: bool, all_features: bool, deny_warnings: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("clippy");

    if all_targets {
        cmd.arg("--all-targets");
    }

    if all_features {
        cmd.arg("--all-features");
    }

    cmd.arg("--");

    if deny_warnings {
        cmd.arg("-D").arg("warnings");
    }

    run_command(cmd, "lint")
}

pub fn format(check: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("fmt");

    if check {
        cmd.arg("--check");
    }

    run_command(cmd, "format")
}
