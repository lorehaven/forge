use crate::config;
use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

pub async fn execute_shell_command(cwd: &Path, args: &Value) -> Result<String> {
    let command = args
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "Missing or invalid 'command' argument. Expected JSON like: {{\"command\":\"anvil lint\"}}"
            )
        })?
        .to_string();

    let command_trim = command.trim();
    let allowed_prefixes = shell_allowed_prefixes();

    let is_allowed = allowed_prefixes.iter().any(|prefix| {
        command_trim.starts_with(prefix) || command_trim.starts_with(&format!("{prefix} "))
    });

    if !is_allowed {
        return Err(anyhow!(
            "Command rejected for safety reasons.\n\nAllowed prefixes:\n  {}\n\nGot: {:?}",
            allowed_prefixes.join("\n  "),
            command_trim
        ));
    }

    let output = tokio::time::timeout(Duration::from_secs(120), async {
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
    })
    .await
    .context("Command execution timed out after 120 seconds")??;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let combined = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        format!("(stderr only)\n{stderr}")
    } else {
        format!("stdout:\n{stdout}\n\nstderr:\n{stderr}")
    };

    let status_msg = if output.status.success() {
        "Command finished successfully".to_string()
    } else {
        format!(
            "Command failed with exit code {}",
            output.status.code().unwrap_or(-1)
        )
    };

    Ok(format!("{status_msg}\n\nOutput:\n{combined}"))
}

fn shell_allowed_prefixes() -> Vec<String> {
    let cfg = config::load();
    if let Some(shell_cfg) = cfg.shell
        && let Some(prefixes) = shell_cfg.allowed_prefixes
    {
        let cleaned: Vec<String> = prefixes
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !cleaned.is_empty() {
            return cleaned;
        }
    }

    default_allowed_prefixes()
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

const fn default_allowed_prefixes() -> &'static [&'static str] {
    &[
        "anvil",
        "anvil lint",
        "anvil build",
        "anvil workspace",
        "anvil docker",
        "cargo check",
        "cargo fmt",
        "cargo clippy",
        "cargo build",
        "cargo run",
        "cargo test",
        "cargo bench",
        "cargo doc",
        "cargo metadata",
        "cargo tree",
        "cargo audit",
        "rustfmt --check",
        "cargo +nightly ",
    ]
}
