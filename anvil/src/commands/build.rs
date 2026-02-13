use anyhow::Result;
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
