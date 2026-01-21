use anyhow::{Context, Result};
use std::process::Command;

use crate::util::run_command;

pub fn list(format: &str) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("metadata")
        .arg("--no-deps")
        .arg("--format-version=1");

    let output = cmd.output().context("Failed to execute cargo metadata")?;

    if !output.status.success() {
        anyhow::bail!("cargo metadata failed");
    }

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse cargo metadata")?;

    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&metadata)?);
        }
        "names" => {
            if let Some(packages) = metadata["packages"].as_array() {
                for package in packages {
                    if let Some(name) = package["name"].as_str() {
                        println!("{}", name);
                    }
                }
            }
        }
        _ => anyhow::bail!("Unknown format: {}", format),
    }

    Ok(())
}

pub fn upgrade(incompatible: bool) -> Result<()> {
    // Check if cargo-upgrade is installed
    which::which("cargo-upgrade")
        .context("cargo-upgrade not found. Install with: cargo install cargo-edit")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("upgrade");

    if incompatible {
        cmd.arg("--incompatible");
    }

    run_command(cmd, "upgrade")
}

pub fn audit() -> Result<()> {
    // Check if cargo-audit is installed
    which::which("cargo-audit")
        .context("cargo-audit not found. Install with: cargo install cargo-audit")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("audit");

    run_command(cmd, "audit")
}

pub fn machete() -> Result<()> {
    // Check if cargo-machete is installed
    which::which("cargo-machete")
        .context("cargo-machete not found. Install with: cargo install cargo-machete")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("machete");

    run_command(cmd, "machete")
}
