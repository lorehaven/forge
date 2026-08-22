use anyhow::Result;
use std::process::Command;

use crate::util::run_command;

#[must_use]
pub fn lint_args(all_targets: bool, all_features: bool, deny_warnings: bool) -> Vec<String> {
    let mut args = vec!["clippy".to_string()];

    if all_targets {
        args.push("--all-targets".to_string());
    }

    if all_features {
        args.push("--all-features".to_string());
    }

    args.push("--".to_string());

    if deny_warnings {
        args.push("-D".to_string());
        args.push("warnings".to_string());
    }

    args
}

pub fn lint(all_targets: bool, all_features: bool, deny_warnings: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args(lint_args(all_targets, all_features, deny_warnings));

    run_command(cmd, "lint")
}

#[must_use]
pub fn format_args(check: bool) -> Vec<String> {
    // Workspaces may rely on unstable rustfmt options (e.g. `group_imports`,
    // `imports_granularity`) in their `rustfmt.toml`, which stable rustfmt
    // silently ignores rather than applying - so it must run under nightly
    // for `--check` to agree with how the code was actually formatted.
    let mut args = vec!["+nightly".to_string(), "fmt".to_string()];

    if check {
        args.push("--check".to_string());
    }

    args
}

pub fn format(check: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args(format_args(check));

    run_command(cmd, "format")
}
