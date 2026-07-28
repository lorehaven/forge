//! The `warehouse` step.
//!
//! Runs `warehouse-cli`, which is how a pipeline puts something into the
//! estate's registries by hand. Collecting a job's declared `artifacts` is a
//! separate mechanism (`crate::artifacts`) that needs no step at all.

use crate::steps::StepError;

/// `warehouse-cli`'s top-level commands, as `cli/warehouse-cli/src/cli.rs`
/// declares them.
pub const COMMANDS: [&str; 4] = ["admin", "crates", "docker", "files"];

/// The second word each of them takes.
const SUBCOMMANDS: [(&str, &[&str]); 4] = [
    ("admin", &["gc"]),
    (
        "crates",
        &["login", "registry", "search", "unyank", "versions", "yank"],
    ),
    ("docker", &["catalog", "login", "registry", "tags"]),
    (
        "files",
        &[
            "bulk-delete",
            "bulk-download",
            "delete",
            "download",
            "ls",
            "mkdir",
            "preview",
            "registry",
            "rmdir",
            "storages",
            "upload",
        ],
    ),
];

pub fn validate(argv: &[String]) -> Result<(), StepError> {
    let command = argv.first().map(String::as_str).unwrap_or_default();

    if !COMMANDS.contains(&command) {
        return Err(StepError::UnknownCommand {
            kind: "warehouse",
            command: command.to_string(),
            known: COMMANDS.join(", "),
        });
    }

    // Every warehouse command is a group; on its own it prints help and exits
    // non-zero, which reads as a mysteriously failing step.
    let Some((_, known)) = SUBCOMMANDS.iter().find(|(name, _)| *name == command) else {
        return Ok(());
    };

    let sub = argv.get(1).map(String::as_str).unwrap_or_default();
    if !known.contains(&sub) {
        return Err(StepError::UnknownCommand {
            kind: "warehouse",
            command: format!("{command} {sub}").trim().to_string(),
            known: known.join(", "),
        });
    }

    Ok(())
}
