use forge_toolbox::{ActionRequest, finish_action, run_selected_action};
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};

fn output(success: bool, stderr: &str) -> Output {
    Output {
        status: ExitStatus::from_raw(if success { 0 } else { 1 }),
        stdout: Vec::new(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

fn req(package: &str, installed: bool) -> ActionRequest {
    ActionRequest {
        package: package.to_string(),
        installable: true,
        installed,
        updatable: true,
    }
}

#[test]
fn finish_action_reports_installed_for_a_fresh_success() {
    let msg = finish_action(&req("anvil", false), output(true, "")).unwrap();
    assert_eq!(msg, "anvil installed");
}

#[test]
fn finish_action_reports_updated_for_a_success_over_an_existing_install() {
    let msg = finish_action(&req("anvil", true), output(true, "")).unwrap();
    assert_eq!(msg, "anvil updated");
}

#[test]
fn finish_action_surfaces_the_first_non_blank_stderr_line_on_failure() {
    let err = finish_action(
        &req("anvil", false),
        output(false, "\n  \nerror: could not find package\nmore detail\n"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "anvil: error: could not find package");
}

#[test]
fn finish_action_falls_back_to_a_generic_message_when_stderr_is_blank() {
    let err = finish_action(&req("anvil", false), output(false, "")).unwrap_err();
    assert_eq!(err.to_string(), "anvil: cargo install failed");
}

// These two branches return before `run_selected_action` shells out to
// `cargo install`, so they're safe to exercise directly in a unit test
// without touching the network or the local cargo install directory.

#[test]
fn run_selected_action_reports_not_installable_without_shelling_out() {
    let req = ActionRequest {
        package: "mystery".to_string(),
        installable: false,
        installed: false,
        updatable: false,
    };
    let msg = run_selected_action(req).unwrap();
    assert_eq!(msg, "mystery is not installable from registry ennor");
}

#[test]
fn run_selected_action_reports_already_up_to_date_without_shelling_out() {
    let req = ActionRequest {
        package: "anvil".to_string(),
        installable: true,
        installed: true,
        updatable: false,
    };
    let msg = run_selected_action(req).unwrap();
    assert_eq!(msg, "anvil is already up to date");
}
