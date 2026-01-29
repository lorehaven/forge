use anyhow::{Context, Result};
use std::process::{Command, Stdio};

pub fn run_command(mut cmd: Command, operation: &str) -> Result<()> {
    println!("Running {operation} operation...");

    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context(format!("Failed to execute {operation} command"))?;

    if !status.success() {
        anyhow::bail!("{operation} operation failed with status: {status}");
    }

    println!("{operation} operation completed successfully");
    Ok(())
}
