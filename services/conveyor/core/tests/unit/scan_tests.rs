use conveyor_core::scan::{
    LEVELS, chip, chosen_levels, counts, has_report_page, severity_class, title_key,
};
use conveyor_core::{CheckChip, ScanFinding};

fn finding(severity: &str) -> ScanFinding {
    ScanFinding {
        severity: severity.into(),
        check: "c".into(),
        service: "s".into(),
        method: "GET".into(),
        path: "/".into(),
        detail: "d".into(),
    }
}

fn check(passed: bool, findings: usize, metric: Option<&str>) -> CheckChip {
    CheckChip {
        kind: "lint".into(),
        passed,
        findings,
        metric: metric.map(Into::into),
    }
}

#[quench_test::test]
fn a_chip_is_coloured_by_what_the_scan_found() {
    let none = chip("lint", None);
    assert_eq!(
        (none.letter, none.class, none.label.as_str()),
        ("L", "chip-none", "-")
    );
    assert_eq!(
        chip("audit", Some(&check(true, 0, None))).class,
        "chip-clean"
    );
    assert_eq!(
        chip("audit", Some(&check(true, 3, None))).class,
        "chip-warning"
    );
    let bad = chip("audit", Some(&check(false, 3, None)));
    assert_eq!((bad.class, bad.label.as_str()), ("chip-danger", "3"));
    let coverage = chip("coverage", Some(&check(true, 50, Some("81%"))));
    assert_eq!(coverage.label, "81%");
    assert_eq!(coverage.title_key, "ui_scan_coverage_title");
}

#[quench_test::test]
fn checks_have_titles_and_only_two_keep_a_report() {
    assert_eq!(title_key("security"), Some("ui_scan_security_title"));
    assert_eq!(title_key("nope"), None);
    assert!(has_report_page("tests") && has_report_page("security"));
    assert!(!has_report_page("lint"));
}

#[quench_test::test]
fn levels_are_counted_and_an_unapplied_filter_shows_all() {
    let all = [
        finding("high"),
        finding("high"),
        finding("info"),
        finding("weird"),
    ];
    assert_eq!(counts(&all), [0, 2, 0, 0, 1]);
    assert_eq!(chosen_levels(&[], false), LEVELS.to_vec());
    assert!(chosen_levels(&[], true).is_empty());
    assert_eq!(chosen_levels(&["HIGH", "low"], true), ["high", "low"]);
}

#[quench_test::test]
fn severities_have_classes() {
    assert_eq!(
        severity_class("yanked"),
        "finding-severity finding-severity-warning"
    );
    assert_eq!(
        severity_class("error"),
        "finding-severity finding-severity-error"
    );
    assert_eq!(severity_class("9.8"), "finding-severity");
}
