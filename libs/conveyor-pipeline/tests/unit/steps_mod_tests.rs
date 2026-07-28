//! Unit tests for `steps/mod.rs` and `steps/shell.rs`.

use conveyor_pipeline::Step;
use conveyor_pipeline::steps::{StepError, argv, shell};

#[test]
fn a_run_step_is_handed_to_a_shell_whole() {
    // Pipelines expect `run` to mean what it says, pipes and redirections
    // included, so it is deliberately not split here.
    let resolved = argv(&Step::Run("cargo build | tee log".to_string())).expect("resolves");
    assert_eq!(resolved[0], shell::SHELL);
    assert_eq!(resolved[1], "-c");
    assert_eq!(resolved[2], "cargo build | tee log");
}

#[test]
fn a_run_step_names_itself_for_shell_error_messages() {
    // `sh -c` assigns the argument after the command to $0, which is what
    // prefixes a "not found" error.
    let resolved = argv(&Step::Run("nope".to_string())).expect("resolves");
    assert_eq!(resolved.get(3).map(String::as_str), Some("conveyor-step"));
}

#[test]
fn tool_steps_resolve_to_the_tool_and_its_arguments() {
    assert_eq!(
        argv(&Step::Anvil("build --all".to_string())).expect("resolves"),
        ["anvil", "build", "--all"]
    );
    assert_eq!(
        argv(&Step::Riveter("apply k8s/".to_string())).expect("resolves"),
        ["riveter", "apply", "k8s/"]
    );
    assert_eq!(
        argv(&Step::Warehouse("publish".to_string())).expect("resolves"),
        ["warehouse-cli", "publish"]
    );
}

#[test]
fn tool_arguments_are_split_without_a_shell() {
    // This is what keeps a value with a space in it from becoming two
    // arguments once something else re-parses the line.
    assert_eq!(
        argv(&Step::Anvil("release --message 'two words'".to_string())).expect("resolves"),
        ["anvil", "release", "--message", "two words"]
    );
}

#[test]
fn a_tool_step_does_not_interpret_shell_syntax() {
    // `;` and `$(...)` are ordinary characters to a tool step. If they were
    // not, a secret injected into an argument could run a second command.
    let resolved = argv(&Step::Anvil("build; rm -rf /".to_string())).expect("resolves");
    assert_eq!(resolved, ["anvil", "build;", "rm", "-rf", "/"]);
}

#[test]
fn unbalanced_quotes_in_a_tool_step_are_rejected() {
    let error = argv(&Step::Anvil("release --message 'unclosed".to_string()))
        .expect_err("should not resolve");
    assert!(matches!(error, StepError::Unparseable { .. }), "{error:?}");
}

#[test]
fn a_tool_step_with_no_arguments_is_rejected() {
    // The parser already refuses an empty command; this covers arguments that
    // are only quoting. `shlex` reads `''` as one empty argument rather than as
    // none, so an emptiness check alone would run the tool with a blank
    // argument instead of reporting the mistake.
    for only_quoting in ["''", "\"\"", "'' \"\""] {
        match argv(&Step::Anvil(only_quoting.to_string())) {
            Err(StepError::NoArguments { .. }) => {}
            other => panic!("{only_quoting:?} should be rejected, got {other:?}"),
        }
    }
}

#[test]
fn an_empty_argument_among_others_is_kept() {
    // `--message ''` is a real thing to want, and rejecting it would be a
    // surprise. Only an argument list that is *entirely* blank is a mistake.
    assert_eq!(
        argv(&Step::Anvil("release --message ''".to_string())).expect("resolves"),
        ["anvil", "release", "--message", ""]
    );
}
