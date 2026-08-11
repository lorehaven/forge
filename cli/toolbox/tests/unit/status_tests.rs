use forge_toolbox::{
    PlannedAction, action_request, build_status, format_toolbox_note, planned_action,
};
use semver::Version;

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

#[test]
fn build_status_not_installed_but_installable() {
    let status = build_status("anvil", "anvil", None, Ok(Some(v("1.2.0"))));
    assert!(status.installable);
    assert!(!status.updatable);
    assert!(status.error.is_none());
    assert_eq!(status.latest_version, Some(v("1.2.0")));
}

#[test]
fn build_status_installed_and_updatable() {
    let status = build_status("anvil", "anvil", Some(v("1.0.0")), Ok(Some(v("1.2.0"))));
    assert!(status.installable);
    assert!(status.updatable);
}

#[test]
fn build_status_installed_and_up_to_date() {
    let status = build_status("anvil", "anvil", Some(v("1.2.0")), Ok(Some(v("1.2.0"))));
    assert!(status.installable);
    assert!(!status.updatable);
}

#[test]
fn build_status_installed_ahead_of_registry_is_not_updatable() {
    // e.g. installed from a git source newer than what's published.
    let status = build_status("anvil", "anvil", Some(v("2.0.0")), Ok(Some(v("1.2.0"))));
    assert!(!status.updatable);
}

#[test]
fn build_status_not_in_registry_is_not_installable() {
    let status = build_status("mystery", "mystery", None, Ok(None));
    assert!(!status.installable);
    assert!(!status.updatable);
    assert!(status.error.is_none());
}

#[test]
fn build_status_lookup_failure_surfaces_as_error_and_disables_actions() {
    let status = build_status(
        "anvil",
        "anvil",
        Some(v("1.0.0")),
        Err(anyhow::anyhow!("registry unreachable")),
    );
    assert!(!status.installable);
    assert!(!status.updatable);
    assert_eq!(status.error.as_deref(), Some("registry unreachable"));
    // installed_version is preserved even when the lookup fails.
    assert_eq!(status.installed_version, Some(v("1.0.0")));
}

#[test]
fn action_request_mirrors_status_fields() {
    let status = build_status("anvil", "anvil", Some(v("1.0.0")), Ok(Some(v("1.2.0"))));
    let req = action_request(&status);
    assert_eq!(req.package, "anvil");
    assert!(req.installable);
    assert!(req.installed);
    assert!(req.updatable);
}

#[test]
fn planned_action_is_none_when_not_installable() {
    let status = build_status("mystery", "mystery", None, Ok(None));
    let req = action_request(&status);
    assert_eq!(planned_action(&req), None);
}

#[test]
fn planned_action_is_install_when_missing_and_installable() {
    let status = build_status("anvil", "anvil", None, Ok(Some(v("1.0.0"))));
    let req = action_request(&status);
    assert_eq!(planned_action(&req), Some(PlannedAction::Install));
}

#[test]
fn planned_action_is_update_when_updatable() {
    let status = build_status("anvil", "anvil", Some(v("1.0.0")), Ok(Some(v("1.2.0"))));
    let req = action_request(&status);
    assert_eq!(planned_action(&req), Some(PlannedAction::Update));
}

#[test]
fn planned_action_is_none_when_up_to_date() {
    let status = build_status("anvil", "anvil", Some(v("1.2.0")), Ok(Some(v("1.2.0"))));
    let req = action_request(&status);
    assert_eq!(planned_action(&req), None);
}

#[test]
fn format_toolbox_note_update_available() {
    let note = format_toolbox_note(Some(v("0.1.0")), Some(v("0.1.5")));
    assert!(note.contains("update available"));
    assert!(note.contains("0.1.0"));
    assert!(note.contains("0.1.5"));
}

#[test]
fn format_toolbox_note_up_to_date() {
    let note = format_toolbox_note(Some(v("0.1.5")), Some(v("0.1.5")));
    assert_eq!(note, "note: forge-toolbox is up to date");
}

#[test]
fn format_toolbox_note_not_installed() {
    let note = format_toolbox_note(None, Some(v("0.1.5")));
    assert_eq!(note, "note: forge-toolbox is not installed (latest 0.1.5)");
}

#[test]
fn format_toolbox_note_unknown_latest() {
    let note = format_toolbox_note(Some(v("0.1.5")), None);
    assert_eq!(
        note,
        "note: could not determine latest forge-toolbox version from registry"
    );
}

#[test]
fn format_toolbox_note_nothing_known() {
    let note = format_toolbox_note(None, None);
    assert_eq!(
        note,
        "note: could not determine latest forge-toolbox version from registry"
    );
}
