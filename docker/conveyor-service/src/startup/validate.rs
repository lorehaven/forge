//! What conveyor needs on `PATH`, reported at startup.
//!
//! Checked here rather than discovered three minutes into somebody's first run,
//! where it costs a checkout and a confusing log line to find out.
//!
//! Nothing here refuses to start. Under `CONVEYOR_EXECUTOR=kubernetes` none of
//! these tools are needed locally at all, and a deployment that has moved to it
//! should not be held up by a check that no longer applies. What a missing tool
//! does cost is loud: an error line naming it, and a run that fails saying the
//! same thing.

use crate::config::{ConveyorConfig, ExecutorKind};

/// Tools a step may call. Absent ones are reported once, at startup, so the
/// operator learns about them before a pipeline does.
const OPTIONAL_TOOLS: [(&str, &str); 5] = [
    ("cargo", "`anvil` build and test steps"),
    ("docker", "`anvil docker` steps"),
    ("kubectl", "`riveter apply` steps"),
    ("anvil", "`anvil` steps"),
    ("riveter", "`riveter` steps"),
];

pub fn report_toolchain(config: &ConveyorConfig) {
    if config.executor != ExecutorKind::Native {
        tracing::info!(
            "executor is {}, so the local toolchain is not used",
            config.executor
        );
        return;
    }

    if which::which("git").is_ok() {
        tracing::info!("git found; checkouts will work");
    } else {
        tracing::error!(
            "git is not on PATH: every run will fail at the checkout. Install it, \
             or move to CONVEYOR_EXECUTOR=kubernetes where each job brings its own."
        );
    }

    let missing: Vec<&str> = OPTIONAL_TOOLS
        .iter()
        .filter(|(tool, _)| which::which(tool).is_err())
        .map(|(tool, needed_for)| {
            tracing::debug!("{tool} not found; {needed_for} will fail");
            *tool
        })
        .collect();

    if !missing.is_empty() {
        tracing::warn!(
            "not on PATH: {}. Pipelines using them will fail; a pipeline of plain \
             `run` steps is unaffected.",
            missing.join(", ")
        );
    }
}
