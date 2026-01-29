use super::utils::{clean_path, resolve_dir};
use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

pub fn git_status(cwd: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("status")
        .arg("--short")
        .current_dir(cwd)
        .output()
        .context("Failed to run git status")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if output.status.success() {
        if stdout.trim().is_empty() {
            Ok("Working tree clean. Nothing to commit.".to_string())
        } else {
            Ok(format!("Git status:\n{stdout}"))
        }
    } else {
        Err(anyhow!(
            "git status failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

pub fn git_diff(cwd: &Path, args: &Value) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("diff");

    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
        let path_clean = clean_path(path);
        let full = resolve_dir(cwd, &path_clean)?;
        cmd.arg(full);
    }

    let output = cmd
        .current_dir(cwd)
        .output()
        .context("Failed to run git diff")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if output.status.success() {
        if stdout.trim().is_empty() {
            Ok("No changes.".to_string())
        } else {
            Ok(format!("Git diff:\n{stdout}"))
        }
    } else {
        Err(anyhow!(
            "git diff failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

pub fn git_add(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let path = clean_path(&raw_path);
    let full = resolve_dir(cwd, &path)?;

    let output = Command::new("git")
        .arg("add")
        .arg(&full)
        .current_dir(cwd)
        .output()
        .context("Failed to run git add")?;

    if output.status.success() {
        Ok(format!("Staged: {path}"))
    } else {
        Err(anyhow!(
            "git add failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

pub fn git_commit(cwd: &Path, args: &Value) -> Result<String> {
    let message: String = serde_json::from_value(args["message"].clone())?;

    if message.trim().is_empty() {
        return Err(anyhow!("Commit message cannot be empty"));
    }

    let output = Command::new("git")
        .arg("commit")
        .arg("-m")
        .arg(&message)
        .current_dir(cwd)
        .output()
        .context("Failed to run git commit")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(format!("Commit created.\n{stdout}"))
    } else {
        Err(anyhow!(
            "git commit failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
