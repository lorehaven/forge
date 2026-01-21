use anyhow::{Context, Result};
use std::process::{Command, Stdio};

pub fn run_command(mut cmd: Command, operation: &str) -> Result<()> {
    println!("Running {} operation...", operation);

    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context(format!("Failed to execute {} command", operation))?;

    if !status.success() {
        anyhow::bail!("{} operation failed with status: {}", operation, status);
    }

    println!("{} operation completed successfully", operation);
    Ok(())
}
