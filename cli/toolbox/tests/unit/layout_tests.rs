use forge_toolbox::{CrateStatus, content_widths, shrink_widths_to_fit, visible_range};
use quench_core::version::Version;

fn status(
    package: &'static str,
    binary: &'static str,
    installed: &str,
    latest: &str,
) -> CrateStatus {
    CrateStatus {
        package: package.to_string(),
        binary: binary.to_string(),
        installed_version: Some(Version::parse(installed).unwrap()),
        latest_version: Some(Version::parse(latest).unwrap()),
        installable: true,
        updatable: false,
        error: None,
    }
}

#[quench_test::test]
fn content_widths_uses_header_length_for_empty_table() {
    let widths = content_widths(&[]);
    // header text + 2 gutter: "Package" = 7 chars, "Installed" = 9 chars, etc.
    assert_eq!(widths.package, 9);
    assert_eq!(widths.installed, 11);
    assert_eq!(widths.status, 8);
}

#[quench_test::test]
fn content_widths_grows_to_fit_longest_package_name() {
    let statuses = vec![status("a-very-long-package-name", "bin", "1.0.0", "1.0.0")];
    let widths = content_widths(&statuses);
    assert_eq!(widths.package, "a-very-long-package-name".len() + 2);
}

#[quench_test::test]
fn shrink_widths_to_fit_noop_when_already_within_budget() {
    let mut widths = content_widths(&[]);
    let before_package = widths.package;
    shrink_widths_to_fit(&mut widths, 500);
    assert_eq!(widths.package, before_package);
}

#[quench_test::test]
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
        + widths.status
        + widths.action
        + 2;

    shrink_widths_to_fit(&mut widths, 62);

    let total_after = widths.package
        + widths.binary
        + widths.installed
        + widths.latest
        + widths.status
        + widths.action
        + 2;
    // The oversized package/binary columns must have been cut down; the
    // algorithm never touches a column that's already at or below its floor.
    assert!(total_after < total_before);
    assert_eq!(widths.package, 9);
    assert_eq!(widths.binary, 9);
}

#[quench_test::test]
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
    // algorithm only ever shrinks, it never grows a column back up. What still
    // does not fit is dropped: the binary and latest columns are hidden.
    assert_eq!(widths.package, 9);
    assert_eq!(widths.binary, 0);
    assert_eq!(widths.latest, 0);
    // The action header is narrower than its floor, and an "Up to date" badge is wider than the status floor.
    assert_eq!(widths.action, 8);
    assert_eq!(widths.status, 14);
    assert!(widths.installed <= 11);
}

#[quench_test::test]
fn shrink_widths_to_fit_hides_binary_before_latest() {
    let statuses = vec![status("anvil", "anvil", "1.0.0", "1.0.0")];
    let mut widths = content_widths(&statuses);
    let floors = 2 + 9 + 9 + 11 + 8 + widths.status.min(14) + widths.action.min(9);
    // Room for every column at its floor but not for the binary one on top.
    shrink_widths_to_fit(&mut widths, floors - 9);
    assert_eq!(widths.binary, 0);
    assert!(widths.latest > 0);
}

#[quench_test::test]
fn a_status_badge_widens_the_status_column() {
    let mut s = status("anvil", "anvil", "1.0.0", "2.0.0");
    s.updatable = true;
    // " ● Update available " = 20 columns + 2 gutter.
    assert_eq!(content_widths(&[s]).status, 22);
}

#[quench_test::test]
fn visible_range_keeps_the_selection_in_view() {
    let statuses: Vec<CrateStatus> = (0..10)
        .map(|n| status("p", "b", &format!("1.0.{n}"), "1.0.0"))
        .collect();
    assert_eq!(visible_range(&statuses, 0, 4), 0..4);
    assert_eq!(visible_range(&statuses, 3, 4), 0..4);
    assert_eq!(visible_range(&statuses, 4, 4), 1..5);
    assert_eq!(visible_range(&statuses, 9, 4), 6..10);
    assert_eq!(visible_range(&statuses, 5, 100), 0..10);
    assert_eq!(visible_range(&[], 0, 4), 0..0);
}

#[quench_test::test]
fn visible_range_counts_an_error_line_as_a_row() {
    let mut statuses: Vec<CrateStatus> =
        (0..4).map(|_| status("p", "b", "1.0.0", "1.0.0")).collect();
    statuses[0].error = Some("boom".to_string());
    // Tool 0 takes two rows, so only two more tools fit in four.
    assert_eq!(visible_range(&statuses, 0, 4), 0..3);
}
