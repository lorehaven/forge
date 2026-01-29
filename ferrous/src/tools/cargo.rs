use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub fn analyze_project(cwd: &Path) -> Result<String> {
    let output = Command::new("cargo")
        .arg("clippy")
        .arg("--all-targets")
        .arg("--")
        .arg("-W")
        .arg("clippy::all")
        .arg("-W")
        .arg("clippy::pedantic")
        .arg("-W")
        .arg("clippy::nursery")
        .current_dir(cwd)
        .output()
        .context("Failed to run cargo clippy")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let status_msg = if output.status.success() {
        "Analysis completed successfully.".to_string()
    } else {
        format!(
            "Analysis completed with warnings (exit code {})",
            output.status.code().unwrap_or(-1)
        )
    };

    let combined = if stdout.trim().is_empty() && stderr.trim().is_empty() {
        "No issues found.".to_string()
    } else if stdout.trim().is_empty() {
        format!("stderr:\n{stderr}")
    } else if stderr.trim().is_empty() {
        format!("stdout:\n{stdout}")
    } else {
        format!("stdout:\n{stdout}\n\nstderr:\n{stderr}")
    };

    Ok(format!("{status_msg}\n\n{combined}"))
}
