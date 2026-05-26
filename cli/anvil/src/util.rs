use anyhow::{Context, Result};
use quench_cli::prelude::{Tone, print_status};
use std::process::{Command, Stdio};

pub fn run_command(mut cmd: Command, operation: &str) -> Result<()> {
    print_status(
        Tone::Info,
        "anvil",
        &format!("running {operation} operation..."),
    );

    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute {operation} command"))?;

    if !status.success() {
        anyhow::bail!("{operation} operation failed with status: {status}");
    }

    print_status(
        Tone::Success,
        "anvil",
        &format!("{operation} operation completed successfully"),
    );
    Ok(())
}
