//! Unit tests for the validated tool steps: `steps/{anvil,riveter,warehouse}.rs`.
//!
//! Validation runs at parse time, so these also assert through
//! `pipeline::parse` where the message an author actually sees is assembled.

use conveyor_service::pipeline::{Step, parse};
use conveyor_service::steps::{StepError, anvil, riveter, validate, warehouse};

fn pipeline_with(step: &str) -> String {
    format!(
        r#"
[[stage]]
name = "build"
[[stage.job]]
name = "the-job"
steps = [{step}]
"#
    )
}

// ---------------------------------------------------------------------------
// anvil
// ---------------------------------------------------------------------------

#[test]
fn every_anvil_command_is_accepted() {
    for command in anvil::COMMANDS {
        // `docker` needs a subcommand of its own; covered separately.
        let args = if command == "docker" {
            "docker build -p thing".to_string()
        } else {
            command.to_string()
        };
        assert!(
            validate(&Step::Anvil(args.clone())).is_ok(),
            "anvil {args} should be accepted"
        );
    }
}

#[test]
fn a_mistyped_anvil_command_is_rejected_with_the_alternatives() {
    let error = validate(&Step::Anvil("buidl --all".to_string())).expect_err("should be rejected");

    let message = error.to_string();
    assert!(message.contains("buidl"), "{message}");
    assert!(message.contains("build"), "{message}");
    assert!(matches!(error, StepError::UnknownCommand { .. }));
}

#[test]
fn anvils_flags_are_not_validated() {
    // Clap already checks them, and a second copy of anvil's argument tables
    // here would be two definitions drifting apart.
    assert!(validate(&Step::Anvil("build --some-new-flag".to_string())).is_ok());
}

#[test]
fn every_anvil_docker_subcommand_is_accepted() {
    for sub in anvil::DOCKER_COMMANDS {
        assert!(
            validate(&Step::Anvil(format!("docker {sub}"))).is_ok(),
            "anvil docker {sub} should be accepted"
        );
    }
}

#[test]
fn a_mistyped_anvil_docker_subcommand_is_rejected() {
    let error =
        validate(&Step::Anvil("docker relase-all".to_string())).expect_err("should be rejected");
    let message = error.to_string();
    assert!(message.contains("relase-all"), "{message}");
    assert!(message.contains("release-all"), "{message}");
}

#[test]
fn anvil_docker_with_no_subcommand_is_rejected() {
    // On its own it prints help and exits non-zero, which reads as a
    // mysteriously failing step.
    assert!(validate(&Step::Anvil("docker".to_string())).is_err());
}

// ---------------------------------------------------------------------------
// riveter
// ---------------------------------------------------------------------------

#[test]
fn every_riveter_command_and_alias_is_accepted() {
    for command in riveter::COMMANDS {
        assert!(
            validate(&Step::Riveter(command.to_string())).is_ok(),
            "riveter {command} should be accepted"
        );
    }
}

#[test]
fn riveters_short_aliases_work() {
    // `a`, `r` and `df` are what people actually type.
    for alias in ["a k8s/", "r --scope all", "df"] {
        assert!(
            validate(&Step::Riveter(alias.to_string())).is_ok(),
            "{alias}"
        );
    }
}

#[test]
fn a_mistyped_riveter_command_is_rejected() {
    let error = validate(&Step::Riveter("aply k8s/".to_string())).expect_err("should be rejected");
    let message = error.to_string();
    assert!(message.contains("aply"), "{message}");
    assert!(message.contains("apply"), "{message}");
}

#[test]
fn riveters_repl_is_rejected_as_interactive() {
    // It waits for input conveyor never sends, so the job would hang until its
    // timeout rather than failing.
    let error = validate(&Step::Riveter("repl".to_string())).expect_err("should be rejected");
    assert!(matches!(error, StepError::Interactive { .. }), "{error:?}");
    assert!(error.to_string().contains("timeout"), "{error}");
}

// ---------------------------------------------------------------------------
// warehouse
// ---------------------------------------------------------------------------

#[test]
fn warehouse_commands_need_a_subcommand() {
    for command in warehouse::COMMANDS {
        assert!(
            validate(&Step::Warehouse(command.to_string())).is_err(),
            "warehouse {command} alone should be rejected"
        );
    }
}

#[test]
fn a_complete_warehouse_command_is_accepted() {
    for args in [
        "files upload artifacts ./thing",
        "crates yank mycrate --version 1.0.0",
        "docker tags forge/sage",
        "admin gc",
    ] {
        assert!(
            validate(&Step::Warehouse(args.to_string())).is_ok(),
            "warehouse {args} should be accepted"
        );
    }
}

#[test]
fn a_mistyped_warehouse_subcommand_is_rejected() {
    let error =
        validate(&Step::Warehouse("files uplaod x y".to_string())).expect_err("should be rejected");
    let message = error.to_string();
    assert!(message.contains("uplaod"), "{message}");
    assert!(message.contains("upload"), "{message}");
}

// ---------------------------------------------------------------------------
// run, and parse-time reporting
// ---------------------------------------------------------------------------

#[test]
fn a_run_step_is_never_validated() {
    // It is the escape hatch, and what a shell will make of a line is not
    // knowable without running it.
    assert!(validate(&Step::Run("anything at all; really".to_string())).is_ok());
    assert!(validate(&Step::Run("buidl".to_string())).is_ok());
}

#[test]
fn a_bad_tool_command_is_a_parse_error_naming_the_stage_and_job() {
    // The whole point of validating here: the author learns about the typo
    // before the checkout, not after the build and test stages have run.
    let error = parse(&pipeline_with(r#"{ riveter = "aply k8s/" }"#))
        .expect_err("should not parse")
        .to_string();

    assert!(error.contains("build"), "{error}");
    assert!(error.contains("the-job"), "{error}");
    assert!(error.contains("aply"), "{error}");
}

#[test]
fn a_good_pipeline_still_parses() {
    let spec = parse(&pipeline_with(
        r#"{ anvil = "build --all" }, { riveter = "apply k8s/" }, { warehouse = "files upload artifacts ./x" }"#,
    ))
    .expect("should parse");

    assert_eq!(spec.stages[0].jobs[0].steps.len(), 3);
}

#[test]
fn the_readme_pipeline_still_parses() {
    // It uses `anvil build --all`, `anvil test --all`, `anvil docker release-all`
    // and `riveter apply k8s/`; validation must not have broken any of them.
    let spec = parse(
        r#"
on = { push = ["master"], pull_request = ["*"] }

[[stage]]
name = "build"
[[stage.job]]
steps = [{ anvil = "build --all" }]

[[stage]]
name  = "test"
needs = ["build"]
[[stage.job]]
steps = [{ anvil = "test --all" }, { run = "cargo fmt --check" }]

[[stage]]
name  = "deploy"
needs = ["test"]
when  = "branch == 'master'"
[[stage.job]]
secrets = ["KUBE_TOKEN"]
steps   = [{ anvil = "docker release-all" }, { riveter = "apply k8s/" }]
"#,
    )
    .expect("the README example must still parse");

    assert_eq!(spec.stages.len(), 3);
}
