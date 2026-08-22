use forge_toolbox::{App, CrateStatus, render};
use semver::Version;

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

fn app(statuses: Vec<CrateStatus>, selected: usize) -> App {
    App {
        selected,
        statuses,
        toolbox_note: "note: forge-toolbox is up to date".to_string(),
        message: "Ready".to_string(),
    }
}

fn rendered(statuses: Vec<CrateStatus>, selected: usize, transient: Option<&str>) -> String {
    render(&app(statuses, selected), transient, 120).join("\n")
}

#[test]
fn header_and_controls_are_always_present() {
    let out = rendered(Vec::new(), 0, None);
    assert!(out.contains("Forge Toolbox Status"));
    assert!(out.contains("registry:"));
    assert!(out.contains("controls:"));
}

#[test]
fn package_and_binary_names_appear_in_their_row() {
    let statuses = vec![CrateStatus {
        package: "anvil",
        binary: "anvil",
        installed_version: Some(v("1.0.0")),
        latest_version: Some(v("1.0.0")),
        installable: true,
        updatable: false,
        error: None,
    }];
    let out = rendered(statuses, 0, None);
    assert!(out.contains("anvil"));
    assert!(out.contains("1.0.0"));
}

#[test]
fn an_updatable_crate_shows_the_update_action() {
    let statuses = vec![CrateStatus {
        package: "riveter",
        binary: "riveter",
        installed_version: Some(v("1.0.0")),
        latest_version: Some(v("2.0.0")),
        installable: true,
        updatable: true,
        error: None,
    }];
    let out = rendered(statuses, 0, None);
    assert!(out.contains("update"));
    assert!(out.contains("yes"));
}

#[test]
fn an_uninstalled_installable_crate_shows_the_install_action() {
    let statuses = vec![CrateStatus {
        package: "welder",
        binary: "welder",
        installed_version: None,
        latest_version: Some(v("1.0.0")),
        installable: true,
        updatable: false,
        error: None,
    }];
    let out = rendered(statuses, 0, None);
    assert!(out.contains("install"));
}

#[test]
fn a_crate_with_an_error_shows_a_warning_line() {
    let statuses = vec![CrateStatus {
        package: "pulley",
        binary: "pulley",
        installed_version: None,
        latest_version: None,
        installable: false,
        updatable: false,
        error: Some("registry lookup failed".to_string()),
    }];
    let out = rendered(statuses, 0, None);
    assert!(out.contains("warn:"));
    assert!(out.contains("registry lookup failed"));
}

#[test]
fn the_selected_row_is_marked() {
    let statuses = vec![
        CrateStatus {
            package: "a",
            binary: "a",
            installed_version: None,
            latest_version: None,
            installable: false,
            updatable: false,
            error: None,
        },
        CrateStatus {
            package: "b",
            binary: "b",
            installed_version: None,
            latest_version: None,
            installable: false,
            updatable: false,
            error: None,
        },
    ];
    let out = render(&app(statuses, 1), None, 120);
    assert!(
        out.iter()
            .any(|line| line.contains('>') && line.contains('b'))
    );
}

#[test]
fn a_transient_status_overrides_the_app_message() {
    let out = rendered(Vec::new(), 0, Some("⠋ running anvil (installing)"));
    assert!(out.contains("running anvil (installing)"));
    assert!(!out.contains("Ready"));
}

#[test]
fn with_no_transient_status_the_app_message_is_shown() {
    let out = rendered(Vec::new(), 0, None);
    assert!(out.contains("Ready"));
}

#[test]
fn a_very_narrow_terminal_still_renders_every_line() {
    let statuses = vec![CrateStatus {
        package: "warehouse-cli",
        binary: "warehouse",
        installed_version: Some(v("1.0.0")),
        latest_version: Some(v("1.2.0")),
        installable: true,
        updatable: true,
        error: None,
    }];
    let lines = render(&app(statuses, 0), None, 20);
    assert!(!lines.is_empty());
}
