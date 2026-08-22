use anyhow::{Context, Result};
use std::process::Command;

use crate::cargo_meta::resolve_package;
use crate::util::run_command;

/// `which::which`, turned into the "not found, here's how to fix it" error.
///
/// Every `pub fn` in this file needs this
/// before shelling out to a cargo subcommand plugin - factored out once so
/// the five call sites don't duplicate the same context-message shape.
pub fn ensure_tool_installed(binary: &str, install_hint: &str) -> Result<()> {
    which::which(binary)
        .map(|_| ())
        .with_context(|| format!("{binary} not found. Install with: {install_hint}"))
}

pub fn format_metadata(format: &str, metadata: &serde_json::Value) -> Result<String> {
    match format {
        "json" => Ok(serde_json::to_string_pretty(metadata)?),
        "names" => {
            let names = metadata["packages"]
                .as_array()
                .map(|pkgs| {
                    pkgs.iter()
                        .filter_map(|p| p["name"].as_str())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(names.join("\n"))
        }
        _ => anyhow::bail!("Unknown format: {format}"),
    }
}

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

    let rendered = format_metadata(format, &metadata)?;
    if !rendered.is_empty() {
        println!("{rendered}");
    }

    Ok(())
}

pub fn upgrade(incompatible: bool) -> Result<()> {
    ensure_tool_installed("cargo-upgrade", "cargo install cargo-edit")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("upgrade");

    if incompatible {
        cmd.arg("--incompatible");
    }

    run_command(cmd, "upgrade")
}

pub fn audit() -> Result<()> {
    ensure_tool_installed("cargo-audit", "cargo install cargo-audit")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("audit");

    run_command(cmd, "audit")
}

pub fn machete() -> Result<()> {
    ensure_tool_installed("cargo-machete", "cargo install cargo-machete")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("machete");

    run_command(cmd, "machete")
}

pub fn deny() -> Result<()> {
    ensure_tool_installed("cargo-deny", "cargo install cargo-deny")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("deny").arg("check");

    run_command(cmd, "deny")
}

/// Finds the commit before the one that last touched a package's `Cargo.toml`.
///
/// That's the state just before its most recent version bump, used as a
/// `cargo semver-checks` baseline when the package isn't fetchable from a
/// public registry.
pub fn previous_version_rev(manifest: &std::path::Path) -> Result<String> {
    let output = Command::new("git")
        .arg("log")
        .arg("--skip=1")
        .arg("-1")
        .arg("--format=%H")
        .arg("--")
        .arg(manifest)
        .output()
        .context("Failed to run git log")?;

    if !output.status.success() {
        anyhow::bail!("git log failed for {}", manifest.display());
    }

    let rev = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if rev.is_empty() {
        anyhow::bail!(
            "No earlier commit found for {} - pass --baseline-rev explicitly",
            manifest.display()
        );
    }

    Ok(rev)
}

pub fn semver_check(package: &str, baseline_rev: Option<String>) -> Result<()> {
    ensure_tool_installed("cargo-semver-checks", "cargo install cargo-semver-checks")?;

    let rev = if let Some(rev) = baseline_rev {
        rev
    } else {
        let pkg = resolve_package(package)?;
        previous_version_rev(&pkg.manifest)?
    };

    let mut cmd = Command::new("cargo");
    cmd.arg("semver-checks")
        .arg("--package")
        .arg(package)
        .arg("--baseline-rev")
        .arg(rev);

    run_command(cmd, "semver-check")
}
