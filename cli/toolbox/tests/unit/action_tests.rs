use forge_toolbox::{ActionRequest, run_selected_action};

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
