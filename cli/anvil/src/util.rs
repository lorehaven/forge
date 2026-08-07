use anyhow::{Context, Result};
use quench_cli::prelude::{DIM, RESET, Tone, print_status};
use std::fs::{self, File};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

/// How many lines of a failed command's log to echo to the terminal. Enough
/// to see the actual compiler/registry error without reprinting an entire
/// multi-thousand-line build log the way `Stdio::inherit()` used to.
const FAILURE_TAIL_LINES: usize = 80;

fn log_file_path(operation: &str) -> PathBuf {
    let slug: String = operation
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "operation" } else { slug };
    PathBuf::from("target/anvil-logs").join(format!("{slug}.log"))
}

pub fn run_command(mut cmd: Command, operation: &str) -> Result<()> {
    print_status(
        Tone::Info,
        "anvil",
        &format!("running {operation} operation..."),
    );

    let log_path = log_file_path(operation);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create log directory {}", parent.display()))?;
    }
    let log_file = File::create(&log_path)
        .with_context(|| format!("Failed to create log file {}", log_path.display()))?;
    let log_file_err = log_file
        .try_clone()
        .with_context(|| format!("Failed to duplicate log handle for {}", log_path.display()))?;

    let start = Instant::now();
    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err))
        .status()
        .with_context(|| format!("Failed to execute {operation} command"))?;
    let elapsed = start.elapsed();

    if !status.success() {
        print_status(
            Tone::Error,
            "anvil",
            &format!("{operation} operation failed with status: {status}"),
        );
        print_log_tail(&log_path);
        anyhow::bail!(
            "{operation} operation failed with status: {status} (full log: {})",
            log_path.display()
        );
    }

    print_status(
        Tone::Success,
        "anvil",
        &format!("{operation} operation completed successfully ({elapsed:.2?})"),
    );
    Ok(())
}

fn print_log_tail(log_path: &PathBuf) {
    let Ok(contents) = fs::read_to_string(log_path) else {
        return;
    };
    let lines: Vec<&str> = contents.lines().collect();
    let start = lines.len().saturating_sub(FAILURE_TAIL_LINES);
    if lines.is_empty() {
        return;
    }

    eprintln!(
        "{DIM}--- last {} line(s) of {} ---{RESET}",
        lines.len() - start,
        log_path.display()
    );
    for line in &lines[start..] {
        eprintln!("{line}");
    }
    eprintln!("{DIM}--- end of log ({}) ---{RESET}", log_path.display());
}
