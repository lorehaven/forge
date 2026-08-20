//! The `anvil` step.
//!
//! Validation is of the command word only, not of every flag. Clap already
//! checks flags, and duplicating its argument tables here would mean two
//! definitions of anvil's interface drifting apart. What clap cannot do is tell
//! an author about the mistake before the run starts - by the time `anvil` is
//! spawned, the checkout has happened and three stages may already have run.

use crate::steps::StepError;

/// Anvil's commands, as `cli/anvil/src/cli.rs` declares them.
///
/// Kept as a list rather than derived, because conveyor and anvil are separate
/// binaries: a deployment can be running a conveyor built against a different
/// anvil. An unknown command here is a warning-shaped error the author can act
/// on, not a wrong answer.
pub const COMMANDS: [&str; 16] = [
    "audit",
    "build",
    "clean",
    "deny",
    "docker",
    "format",
    "install",
    "lint",
    "list",
    "machete",
    "nextest",
    "release",
    "run",
    "semver-check",
    "test",
    "upgrade",
];

/// `anvil docker`'s own subcommands.
pub const DOCKER_COMMANDS: [&str; 6] = [
    "build",
    "build-all",
    "push",
    "release",
    "release-all",
    "tag",
];

/// Checks an `anvil` step's command word (and, for `docker`, its subcommand)
/// against [`COMMANDS`]/[`DOCKER_COMMANDS`].
pub fn validate(argv: &[String]) -> Result<(), StepError> {
    let command = argv.first().map(String::as_str).unwrap_or_default();

    if !COMMANDS.contains(&command) {
        return Err(StepError::UnknownCommand {
            kind: "anvil",
            command: command.to_string(),
            known: COMMANDS.join(", "),
        });
    }

    if command == "docker" {
        let sub = argv.get(1).map(String::as_str).unwrap_or_default();
        if !DOCKER_COMMANDS.contains(&sub) {
            return Err(StepError::UnknownCommand {
                kind: "anvil docker",
                command: sub.to_string(),
                known: DOCKER_COMMANDS.join(", "),
            });
        }
    }

    Ok(())
}
