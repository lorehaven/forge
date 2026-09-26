use anyhow::{Context, Result};
use std::io::Write;
use std::process::Command;

use crate::cargo_meta::resolve_package;
use crate::util::{run_command, run_command_json, run_command_streamed};

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

pub fn audit(json: bool) -> Result<()> {
    ensure_tool_installed("cargo-audit", "cargo install cargo-audit")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("audit");

    if json {
        // cargo-audit's own flag: the report goes to stdout, the advisory-db
        // fetch's own progress chatter to stderr.
        cmd.arg("--format").arg("json");
        return run_command_json(cmd);
    }

    // Streamed, not captured: the advisory report is the whole point, and a
    // long one must not lose its top rows to the failure tail.
    run_command_streamed(cmd, "audit")
}

/// One crate's unused dependencies, as `cargo-machete`'s plain-text report names it.
///
/// cargo-machete has no `--json` of its own (checked against its 0.9.2
/// `--help`), so `machete`'s `--json` mode parses this out of its stdout
/// instead of asking it for structured output directly.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MacheteFinding {
    pub package: String,
    pub manifest: String,
    pub unused: Vec<String>,
}

/// Parses `cargo-machete`'s stdout, e.g.:
///
/// ```text
/// cargo-machete found the following unused dependencies in this directory:
/// workbench-service -- ./docker/workbench-service/Cargo.toml:
///     rustls
/// ```
///
/// A `"cargo-machete didn't find any unused dependencies..."` clean report,
/// or output that doesn't match either shape, both parse to an empty `Vec`.
#[must_use]
pub fn parse_machete_output(stdout: &str) -> Vec<MacheteFinding> {
    let mut findings = Vec::new();
    let mut current: Option<MacheteFinding> = None;

    for line in stdout.lines() {
        if let Some(dep) = line.strip_prefix('\t') {
            if let Some(finding) = current.as_mut() {
                finding.unused.push(dep.trim().to_string());
            }
            continue;
        }

        if let Some(header) = line.strip_suffix(':')
            && let Some((package, manifest)) = header.split_once(" -- ")
        {
            if let Some(finding) = current.take() {
                findings.push(finding);
            }
            current = Some(MacheteFinding {
                package: package.to_string(),
                manifest: manifest.to_string(),
                unused: Vec::new(),
            });
            continue;
        }

        if let Some(finding) = current.take() {
            findings.push(finding);
        }
    }
    if let Some(finding) = current.take() {
        findings.push(finding);
    }

    findings
}

/// `machete`'s `--json` report: always exactly one line.
///
/// Even when `findings` is empty, so "no unused dependencies" and "nothing
/// recognised at all" (a consumer's own JSON parse failing outright) stay
/// distinguishable, the same way cargo-audit's own single-object `--json`
/// report already is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MacheteReport {
    pub findings: Vec<MacheteFinding>,
}

pub fn machete(json: bool) -> Result<()> {
    ensure_tool_installed("cargo-machete", "cargo install cargo-machete")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("machete");

    if json {
        let output = cmd.output().context("Failed to execute machete command")?;
        std::io::stderr()
            .write_all(&output.stderr)
            .context("Failed to write machete's progress output")?;

        let findings = parse_machete_output(&String::from_utf8_lossy(&output.stdout));
        println!("{}", serde_json::to_string(&MacheteReport { findings })?);

        if !output.status.success() {
            anyhow::bail!("machete operation failed with status: {}", output.status);
        }
        return Ok(());
    }

    // Streamed, not captured: the list of unused dependencies and the paths
    // they were found in need to come back in full and in color.
    run_command_streamed(cmd, "machete")
}

pub fn deny(json: bool) -> Result<()> {
    ensure_tool_installed("cargo-deny", "cargo install cargo-deny")?;

    let mut cmd = Command::new("cargo");
    cmd.arg("deny");
    if json {
        cmd.arg("--format").arg("json");
    }
    cmd.arg("check");

    if json {
        // cargo-deny writes its `--format json` diagnostics to stderr, not
        // stdout (checked against a real run - stdout stayed empty), since
        // stdout is reserved for other subcommands like `list`. Swap the
        // streams so `--json` means the same thing here as everywhere else
        // in anvil: the report is on anvil's own stdout.
        let output = cmd.output().context("Failed to execute deny command")?;
        std::io::stdout()
            .write_all(&output.stderr)
            .context("Failed to write deny's JSON report")?;
        std::io::stderr()
            .write_all(&output.stdout)
            .context("Failed to write deny's stdout")?;

        if !output.status.success() {
            anyhow::bail!("deny operation failed with status: {}", output.status);
        }
        return Ok(());
    }

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
