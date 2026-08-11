use forge_toolbox::{CrateStatus, content_widths, shrink_widths_to_fit};
use semver::Version;

fn status(
    package: &'static str,
    binary: &'static str,
    installed: &str,
    latest: &str,
) -> CrateStatus {
    CrateStatus {
        package,
        binary,
        installed_version: Some(Version::parse(installed).unwrap()),
        latest_version: Some(Version::parse(latest).unwrap()),
        installable: true,
        updatable: false,
        error: None,
    }
}

#[test]
fn content_widths_uses_header_length_for_empty_table() {
    let widths = content_widths(&[]);
    // header text + 2 padding: "package" = 7 chars, "installed" = 9 chars, etc.
    assert_eq!(widths.package, 9);
    assert_eq!(widths.installed, 11);
    assert_eq!(widths.updatable, 11);
}

#[test]
fn content_widths_grows_to_fit_longest_package_name() {
    let statuses = vec![status("a-very-long-package-name", "bin", "1.0.0", "1.0.0")];
    let widths = content_widths(&statuses);
    assert_eq!(widths.package, "a-very-long-package-name".len() + 2);
}

#[test]
fn shrink_widths_to_fit_noop_when_already_within_budget() {
    let mut widths = content_widths(&[]);
    let before_package = widths.package;
    shrink_widths_to_fit(&mut widths, 500);
    assert_eq!(widths.package, before_package);
}

#[test]
fn shrink_widths_to_fit_reduces_widest_columns_first() {
    let statuses = vec![status(
        "a-very-extremely-long-package-name",
        "a-very-extremely-long-binary-name",
        "1.0.0",
        "1.0.0",
    )];
    let mut widths = content_widths(&statuses);
    let total_before = widths.package
        + widths.binary
        + widths.installed
        + widths.latest
        + widths.updatable
        + widths.action
        + 10;

    shrink_widths_to_fit(&mut widths, 40);

    let total_after = widths.package
        + widths.binary
        + widths.installed
        + widths.latest
        + widths.updatable
        + widths.action
        + 10;
    // The oversized package/binary columns must have been cut down; the
    // algorithm never touches a column that's already at or below its floor.
    assert!(total_after < total_before);
    assert_eq!(widths.package, 7);
    assert_eq!(widths.binary, 7);
}

#[test]
fn shrink_widths_to_fit_stops_at_minimums_and_does_not_underflow() {
    let statuses = vec![status(
        "a-very-extremely-long-package-name",
        "a-very-extremely-long-binary-name",
        "1.0.0",
        "1.0.0",
    )];
    let mut widths = content_widths(&statuses);

    // An impossibly narrow terminal must not panic (usize underflow) or loop forever.
    shrink_widths_to_fit(&mut widths, 1);

    // Every column settles at its configured minimum, except columns whose
    // natural (header-driven) width was already below that minimum - the
    // algorithm only ever shrinks, it never grows a column back up.
    assert_eq!(widths.package, 7);
    assert_eq!(widths.binary, 7);
    assert_eq!(widths.latest, 6);
    assert_eq!(widths.action, 6);
    assert_eq!(widths.updatable, 9);
    assert!(widths.installed <= 12);
}
