use conveyor_service::scan::{
    CheckKind, CheckResult, parse_audit, parse_coverage, parse_lint, parse_machete, strip_ansi,
};

#[test]
fn strips_sgr_sequences() {
    assert_eq!(
        strip_ansi("\u{1b}[33mwarning\u{1b}[0m: unused"),
        "warning: unused"
    );
    assert_eq!(strip_ansi("plain text"), "plain text");
}

#[test]
fn parses_clean_lint() {
    assert!(parse_lint(&["Checking foo v0.1.0", "Finished"]).is_none());
}

/// Regression: a passed lint step whose output doesn't match cargo's
/// `warning:`/`error:` shape (e.g. anvil's own "N modules scanned"
/// summary) must not have its log tail treated as findings - that
/// previously made a clean run's chip show the line count instead of 0.
#[test]
fn passed_check_with_unparseable_output_has_no_findings() {
    let result = CheckResult {
        kind: CheckKind::Lint,
        job_name: "check/lint".to_string(),
        headline: String::new(),
        findings: Vec::new(),
        metric: None,
        passed: true,
    }
    .parsed(&["10 modules scanned, 0 lint errors"], Some(0));

    assert_eq!(result.headline, "passed");
    assert!(result.findings.is_empty());
}

#[test]
fn failed_check_with_unparseable_output_falls_back_to_log_tail() {
    let result = CheckResult {
        kind: CheckKind::Lint,
        job_name: "check/lint".to_string(),
        headline: String::new(),
        findings: Vec::new(),
        metric: None,
        passed: false,
    }
    .parsed(&["thread panicked", "some other diagnostic"], Some(101));

    assert_eq!(result.headline, "failed (exit 101)");
    assert_eq!(result.findings.len(), 2);
    assert_eq!(result.findings[0].title, "thread panicked");
}

#[test]
fn parses_lint_warnings_with_location() {
    let lines = [
        "warning: unused variable: `x`",
        "  --> src/main.rs:10:9",
        "warning: unused import",
    ];
    let (headline, findings) = parse_lint(&lines).expect("should parse");
    assert_eq!(headline, "2 warnings");
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].title, "unused variable: `x`");
    assert_eq!(findings[0].severity.as_deref(), Some("warning"));
    assert_eq!(findings[0].location.as_deref(), Some("src/main.rs:10:9"));
    assert_eq!(findings[1].location, None);
}

#[test]
fn parses_bracketed_lint_diagnostics() {
    let lines = ["error[E0308]: mismatched types"];
    let (headline, findings) = parse_lint(&lines).expect("should parse");
    assert_eq!(headline, "1 error");
    assert_eq!(findings[0].title, "mismatched types");
    assert_eq!(findings[0].severity.as_deref(), Some("error"));
}

#[test]
fn parses_clean_machete() {
    let lines = ["cargo-machete didn't find any unused dependencies in this directory. Good job!"];
    let (headline, findings) = parse_machete(&lines).expect("should parse");
    assert_eq!(headline, "clean");
    assert!(findings.is_empty());
}

#[test]
fn parses_machete_findings() {
    let lines = ["foo -- ./crates/foo:", "    serde_yaml", "    once_cell"];
    let (headline, findings) = parse_machete(&lines).expect("should parse");
    assert_eq!(headline, "2 unused dependencies");
    assert_eq!(findings[0].title, "serde_yaml");
    assert_eq!(findings[0].location.as_deref(), Some("foo"));
    assert_eq!(findings[1].title, "once_cell");
}

#[test]
fn parses_clean_audit() {
    let lines = ["Scanning Cargo.lock", "0 vulnerabilities found"];
    let (headline, findings) = parse_audit(&lines).expect("should parse");
    assert_eq!(headline, "clean");
    assert!(findings.is_empty());
}

#[test]
fn parses_audit_findings_with_all_fields() {
    let lines = [
        "Crate:     time",
        "Version:   0.1.43",
        "Title:     Potential segfault",
        "Date:      2020-11-18",
        "ID:        RUSTSEC-2020-0071",
        "Severity:  6.2 (medium)",
        "Solution:  Upgrade to >=0.2.23",
    ];
    let (headline, findings) = parse_audit(&lines).expect("should parse");
    assert_eq!(headline, "1 finding");
    let finding = &findings[0];
    assert_eq!(finding.title, "Potential segfault");
    assert_eq!(finding.id.as_deref(), Some("RUSTSEC-2020-0071"));
    assert_eq!(finding.date.as_deref(), Some("2020-11-18"));
    assert_eq!(finding.severity.as_deref(), Some("6.2 (medium)"));
    assert_eq!(finding.location.as_deref(), Some("time 0.1.43"));
    assert_eq!(
        finding.extra.as_deref(),
        Some("Solution: Upgrade to >=0.2.23")
    );
}

#[test]
fn parses_multiple_audit_blocks_separated_by_blank_lines() {
    let lines = [
        "Crate:     rsa",
        "Version:   0.9.10",
        "Title:     Marvin Attack",
        "Date:      2023-11-22",
        "ID:        RUSTSEC-2023-0071",
        "Severity:  5.9 (medium)",
        "Solution:  No fixed upgrade is available!",
        "",
        "Crate:     proc-macro-error2",
        "Version:   2.0.1",
        "Warning:   unmaintained",
        "Title:     proc-macro-error2 is unmaintained",
        "Date:      2026-06-07",
        "ID:        RUSTSEC-2026-0173",
    ];
    let (headline, findings) = parse_audit(&lines).expect("should parse");
    assert_eq!(headline, "2 findings");
    assert_eq!(findings[0].id.as_deref(), Some("RUSTSEC-2023-0071"));
    assert_eq!(findings[1].id.as_deref(), Some("RUSTSEC-2026-0173"));
    assert_eq!(findings[1].severity.as_deref(), Some("unmaintained"));
}

#[test]
fn parses_coverage_table_and_skips_fully_covered_files() {
    let lines = [
        "Filename                      Regions    Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover    Branches   Missed Branches     Cover",
        "-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------",
        "src/lib.rs                        119                99    16.81%          15                12    20.00%          83                69    16.87%           0                 0         -",
        "src/fully_covered.rs                10                 0   100.00%           2                 0   100.00%          10                 0   100.00%           0                 0         -",
        "src/small_gap.rs                    50                 5    90.00%           4                 1    75.00%          40                 2    95.00%           0                 0         -",
        "-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------",
        "TOTAL                              179               104    41.90%          21                13    38.10%         133                71    46.62%           0                 0         -",
    ];
    let (headline, findings, metric) = parse_coverage(&lines).expect("should parse");
    assert_eq!(headline, "46.62% line coverage");
    assert_eq!(metric, "47%");
    // fully_covered.rs missed nothing - it is not a finding.
    assert_eq!(findings.len(), 2);
    // Worst first: 69 missed lines beats 2, regardless of percentage.
    assert_eq!(findings[0].title, "src/lib.rs");
    assert_eq!(findings[0].severity.as_deref(), Some("16.87%"));
    assert_eq!(
        findings[0].location.as_deref(),
        Some("69 of 83 lines missed")
    );
    assert_eq!(findings[1].title, "src/small_gap.rs");
    assert_eq!(findings[1].severity.as_deref(), Some("95.00%"));
}

#[test]
fn a_coverage_report_with_no_recognisable_rows_does_not_parse() {
    assert!(parse_coverage(&["error: no coverage data found"]).is_none());
}
