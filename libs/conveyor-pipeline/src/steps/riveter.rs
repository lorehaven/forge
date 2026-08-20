//! The `riveter` step.
//!
//! Riveter talks to a cluster, so a mistyped command here is the one most worth
//! catching early: `aply` would fail the deploy stage after the build and test
//! stages had already spent their time.

use crate::steps::StepError;

/// Riveter's commands and their aliases, as `cli/riveter/src/cli.rs` declares
/// them.
pub const COMMANDS: [&str; 15] = [
    "a", "apply", "d", "del", "delete", "df", "diff", "env", "help", "h", "list", "ls", "prune",
    "r", "render",
];

/// `repl` is deliberately absent. It waits for input conveyor will never send,
/// so a pipeline that asks for it hangs until the job's timeout.
const INTERACTIVE: [&str; 1] = ["repl"];

/// Checks a `riveter` step's command word against [`COMMANDS`], and rejects
/// [`INTERACTIVE`] ones outright.
pub fn validate(argv: &[String]) -> Result<(), StepError> {
    let command = argv.first().map(String::as_str).unwrap_or_default();

    if INTERACTIVE.contains(&command) {
        return Err(StepError::Interactive {
            kind: "riveter",
            command: command.to_string(),
        });
    }

    if !COMMANDS.contains(&command) {
        return Err(StepError::UnknownCommand {
            kind: "riveter",
            command: command.to_string(),
            known: COMMANDS.join(", "),
        });
    }

    Ok(())
}
