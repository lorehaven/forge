//! The `riveter` step.
//!
//! Riveter talks to a cluster, so a mistyped command here is the one most worth
//! catching early: `aply` would fail the deploy stage after the build and test
//! stages had already spent their time.

use crate::steps::StepError;

/// Riveter's commands and their aliases, as `cli/riveter/src/cli.rs` declares
/// them.
pub const COMMANDS: [&str; 24] = [
    "a", "apply", "d", "del", "delete", "df", "diff", "env", "h", "help", "i", "images", "install",
    "list", "ls", "pack", "prune", "publish", "pull", "r", "remote", "render", "schemas",
    "validate",
];

/// `repl` is deliberately absent. It waits for input conveyor will never send,
/// so a pipeline that asks for it hangs until the job's timeout.
const INTERACTIVE: [&str; 1] = ["repl"];

/// The command word, past any leading `--env <name>` / `-e <name>`.
///
/// Riveter's `--env` is a global option that comes first, and it is what a pipeline
/// uses to pick the overlay (`--env forge pack`), so the first argument is not
/// necessarily the command.
fn command_word(argv: &[String]) -> &str {
    let mut arguments = argv.iter().map(String::as_str);

    while let Some(argument) = arguments.next() {
        match argument {
            "--env" | "-e" => {
                arguments.next();
            }
            _ if argument.starts_with("--env=") => {}
            _ if argument.len() > 2
                && argument.starts_with("-e")
                && !argument.starts_with("--") => {}
            command => return command,
        }
    }

    ""
}

/// Checks a `riveter` step's command word against [`COMMANDS`], and rejects
/// [`INTERACTIVE`] ones outright.
pub fn validate(argv: &[String]) -> Result<(), StepError> {
    let command = command_word(argv);

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
