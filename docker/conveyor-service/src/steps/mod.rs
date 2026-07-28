//! Turning a pipeline step into something an executor can spawn.
//!
//! Only `run` goes through a shell. The tool steps are split into an argument
//! vector here, which keeps a value with a space or a quote in it from being
//! re-parsed by `sh` into two arguments - and is why a secret injected into an
//! `anvil` step cannot rewrite the command it appears in.
//!
//! Each tool step's command word is checked against what that tool actually
//! accepts. [`validate`] is called by the pipeline parser, so a mistyped
//! command is a parse error naming the stage and job at fault rather than a
//! deploy stage that fails after the build and test stages have already spent
//! their time.

use crate::pipeline::Step;

pub mod anvil;
pub mod riveter;
pub mod shell;
pub mod warehouse;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StepError {
    #[error("`{kind}` arguments are not quoted correctly: {command}")]
    Unparseable { kind: &'static str, command: String },

    #[error("`{kind}` has no arguments")]
    NoArguments { kind: &'static str },

    #[error("`{kind}` has no command `{command}` (known: {known})")]
    UnknownCommand {
        kind: &'static str,
        command: String,
        known: String,
    },

    #[error(
        "`{kind} {command}` waits for input, which conveyor never sends; \
         the job would hang until its timeout"
    )]
    Interactive { kind: &'static str, command: String },
}

/// The program and arguments to spawn for `step`.
pub fn argv(step: &Step) -> Result<Vec<String>, StepError> {
    match step {
        Step::Run(command) => Ok(shell::argv(command)),
        Step::Anvil(args) => tool("anvil", args),
        Step::Riveter(args) => tool("riveter", args),
        Step::Warehouse(args) => tool("warehouse-cli", args),
    }
}

/// Checks a step without spawning anything.
///
/// A `run` step is not checked: it is the escape hatch, and what a shell will
/// make of a line is not knowable without running it.
pub fn validate(step: &Step) -> Result<(), StepError> {
    let arguments = match step {
        Step::Run(_) => return Ok(()),
        Step::Anvil(args) => split("anvil", args)?,
        Step::Riveter(args) => split("riveter", args)?,
        Step::Warehouse(args) => split("warehouse-cli", args)?,
    };

    match step {
        Step::Run(_) => Ok(()),
        Step::Anvil(_) => anvil::validate(&arguments),
        Step::Riveter(_) => riveter::validate(&arguments),
        Step::Warehouse(_) => warehouse::validate(&arguments),
    }
}

/// Splits a tool step's arguments the way a shell would, without a shell.
fn split(program: &'static str, args: &str) -> Result<Vec<String>, StepError> {
    let split = shlex::split(args).ok_or_else(|| StepError::Unparseable {
        kind: program,
        command: args.to_string(),
    })?;

    // `shlex::split("''")` is one empty argument, not zero arguments, so an
    // emptiness check alone would let `anvil ''` through and run the tool with
    // a blank argument. An empty argument among others is fine and sometimes
    // meant - `--message ''` - which is why this asks whether *every* one is
    // blank rather than whether any is.
    if split.iter().all(|argument| argument.trim().is_empty()) {
        return Err(StepError::NoArguments { kind: program });
    }

    Ok(split)
}

fn tool(program: &'static str, args: &str) -> Result<Vec<String>, StepError> {
    let split = split(program, args)?;

    let mut argv = Vec::with_capacity(split.len() + 1);
    argv.push(program.to_string());
    argv.extend(split);
    Ok(argv)
}
