use conveyor_service::scan::{
    CheckKind, CheckResult, parse_audit, parse_coverage, parse_lint, parse_machete, strip_ansi,
    security_check, tests_check,
};

#[quench_test::test]
fn strips_sgr_sequences() {
    assert_eq!(
        strip_ansi("\u{1b}[33mwarning\u{1b}[0m: unused"),
        "warning: unused"
    );
    assert_eq!(strip_ansi("plain text"), "plain text");
}

#[quench_test::test]
fn parses_clean_lint() {
    assert!(parse_lint(&["Checking foo v0.1.0", "Finished"]).is_none());
}

/// Regression: a passed lint step whose output doesn't match cargo's
/// `warning:`/`error:` shape (e.g. anvil's own "N modules scanned"
/// summary) must not have its log tail treated as findings - that
/// previously made a clean run's chip show the line count instead of 0.
#[quench_test::test]
fn passed_check_with_unparseable_output_has_no_findings() {
    let result = CheckResult {
        kind: CheckKind::Lint,
        job_name: "check/lint".to_string(),
        headline: String::new(),
        findings: Vec::new(),
        metric: None,
        report_uri: None,
        passed: true,
    }
    .parsed(&["10 modules scanned, 0 lint errors"], Some(0));

    assert_eq!(result.headline, "passed");
    assert!(result.findings.is_empty());
}

#[quench_test::test]
fn failed_check_with_unparseable_output_falls_back_to_log_tail() {
    let result = CheckResult {
        kind: CheckKind::Lint,
        job_name: "check/lint".to_string(),
        headline: String::new(),
        findings: Vec::new(),
        metric: None,
        report_uri: None,
        passed: false,
    }
    .parsed(&["thread panicked", "some other diagnostic"], Some(101));

    assert_eq!(result.headline, "failed (exit 101)");
    assert_eq!(result.findings.len(), 2);
    assert_eq!(result.findings[0].title, "thread panicked");
}

#[quench_test::test]
fn parses_lint_warnings_with_location() {
    let lines = [
        r#"{"reason":"compiler-message","message":{"level":"warning","message":"unused variable: `x`","spans":[{"file_name":"src/main.rs","line_start":10,"column_start":9,"is_primary":true}]}}"#,
        r#"{"reason":"compiler-message","message":{"level":"warning","message":"unused import","spans":[]}}"#,
    ];
    let (headline, findings) = parse_lint(&lines).expect("should parse");
    assert_eq!(headline, "2 warnings");
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].title, "unused variable: `x`");
    assert_eq!(findings[0].severity.as_deref(), Some("warning"));
    assert_eq!(findings[0].location.as_deref(), Some("src/main.rs:10:9"));
    assert_eq!(findings[1].location, None);
}

#[quench_test::test]
fn parses_error_level_lint_diagnostics() {
    let lines = [
        r#"{"reason":"compiler-message","message":{"level":"error","message":"mismatched types","code":{"code":"E0308"},"spans":[]}}"#,
    ];
    let (headline, findings) = parse_lint(&lines).expect("should parse");
    assert_eq!(headline, "1 error");
    assert_eq!(findings[0].title, "mismatched types");
    assert_eq!(findings[0].severity.as_deref(), Some("error"));
}

#[quench_test::test]
fn parse_lint_recognises_a_clean_build_and_ignores_notes() {
    let lines = [
        r#"{"reason":"compiler-artifact"}"#,
        r#"{"reason":"compiler-message","message":{"level":"note","message":"ignore this"}}"#,
        r#"{"reason":"build-finished","success":true}"#,
    ];
    let (headline, findings) = parse_lint(&lines).expect("should parse");
    assert_eq!(headline, "clean");
    assert!(findings.is_empty());
}

#[quench_test::test]
fn parses_clean_machete() {
    let lines = [r#"{"findings":[]}"#];
    let (headline, findings) = parse_machete(&lines).expect("should parse");
    assert_eq!(headline, "clean");
    assert!(findings.is_empty());
}

#[quench_test::test]
fn parses_machete_findings() {
    let lines = [
        r#"{"findings":[{"package":"foo","manifest":"./crates/foo/Cargo.toml","unused":["serde_yaml","once_cell"]}]}"#,
    ];
    let (headline, findings) = parse_machete(&lines).expect("should parse");
    assert_eq!(headline, "2 unused dependencies");
    assert_eq!(findings[0].title, "serde_yaml");
    assert_eq!(findings[0].location.as_deref(), Some("foo"));
    assert_eq!(findings[1].title, "once_cell");
}

#[quench_test::test]
fn parse_machete_is_none_for_output_that_is_not_its_json() {
    assert!(parse_machete(&["Analyzing dependencies of crates in this directory..."]).is_none());
}

#[quench_test::test]
fn parses_clean_audit() {
    let lines = [r#"{"vulnerabilities":{"found":false,"count":0,"list":[]},"warnings":{}}"#];
    let (headline, findings) = parse_audit(&lines).expect("should parse");
    assert_eq!(headline, "clean");
    assert!(findings.is_empty());
}

#[quench_test::test]
fn parses_audit_findings_with_all_fields() {
    let lines = [concat!(
        r#"{"vulnerabilities":{"found":true,"count":1,"list":[{"#,
        r#""advisory":{"id":"RUSTSEC-2020-0071","title":"Potential segfault","date":"2020-11-18","#,
        r#""cvss":"CVSS:3.1/AV:L/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H","informational":null},"#,
        r#""package":{"name":"time","version":"0.1.43"},"#,
        r#""versions":{"patched":[">=0.2.23"],"unaffected":[]}"#,
        r#"}]},"warnings":{}}"#
    )];
    let (headline, findings) = parse_audit(&lines).expect("should parse");
    assert_eq!(headline, "1 finding");
    let finding = &findings[0];
    assert_eq!(finding.title, "Potential segfault");
    assert_eq!(finding.id.as_deref(), Some("RUSTSEC-2020-0071"));
    assert_eq!(finding.date.as_deref(), Some("2020-11-18"));
    assert_eq!(
        finding.severity.as_deref(),
        Some("CVSS:3.1/AV:L/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H")
    );
    assert_eq!(finding.location.as_deref(), Some("time 0.1.43"));
    assert_eq!(
        finding.extra.as_deref(),
        Some("Solution: Upgrade to >=0.2.23")
    );
}

#[quench_test::test]
fn parses_vulnerabilities_and_warnings_together() {
    let lines = [concat!(
        r#"{"vulnerabilities":{"found":true,"count":1,"list":[{"#,
        r#""advisory":{"id":"RUSTSEC-2023-0071","title":"Marvin Attack","date":"2023-11-22","#,
        r#""cvss":"CVSS:3.1/AV:N/AC:H/PR:N/UI:N/S:U/C:H/I:N/A:N","informational":null},"#,
        r#""package":{"name":"rsa","version":"0.9.10"},"versions":{"patched":[],"unaffected":[]}}]},"#,
        r#""warnings":{"unmaintained":[{"#,
        r#""advisory":{"id":"RUSTSEC-2026-0173","title":"proc-macro-error2 is unmaintained","#,
        r#""date":"2026-06-07","informational":"unmaintained"},"#,
        r#""package":{"name":"proc-macro-error2","version":"2.0.1"},"versions":{"patched":[],"unaffected":[]}}]}}"#
    )];
    let (headline, findings) = parse_audit(&lines).expect("should parse");
    assert_eq!(headline, "2 findings");
    assert_eq!(findings[0].id.as_deref(), Some("RUSTSEC-2023-0071"));
    assert_eq!(findings[1].id.as_deref(), Some("RUSTSEC-2026-0173"));
    assert_eq!(findings[1].severity.as_deref(), Some("unmaintained"));
}

#[quench_test::test]
fn parse_audit_is_none_for_output_that_is_not_its_json() {
    assert!(parse_audit(&["Fetching advisory database..."]).is_none());
}

#[quench_test::test]
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

#[quench_test::test]
fn a_coverage_report_with_no_recognisable_rows_does_not_parse() {
    assert!(parse_coverage(&["error: no coverage data found"]).is_none());
}

const TESTS_REPORT: &str = r#"{
  "title": "All test results", "started": 1, "duration_ms": 95000,
  "summary": {"total": 5, "passed": 3, "failed": 1, "skipped": 1},
  "suites": [
    {"name": "api", "kind": "unit", "cases": [
      {"name": "api::ok", "status": "passed", "duration_ms": 1},
      {"name": "api::broken", "status": "failed", "duration_ms": 2,
       "message": "assertion failed: left == right", "location": "tests/api.rs:12"},
      {"name": "api::later", "status": "skipped", "duration_ms": 0}
    ]},
    {"name": "Checkout", "kind": "bdd", "cases": [
      {"name": "pay", "status": "failed", "duration_ms": 3},
      {"name": "refund", "status": "passed", "duration_ms": 4},
      {"name": "ship", "status": "passed", "duration_ms": 4}
    ]}
  ]
}"#;

#[quench_test::test]
fn a_test_report_becomes_a_check_with_each_failed_test_a_finding() {
    let check = tests_check("test/unit", TESTS_REPORT).expect("a report");

    assert_eq!(check.kind, CheckKind::Tests);
    assert_eq!(check.job_name, "test/unit");
    assert!(!check.passed);
    assert_eq!(check.headline, "3 passed, 1 failed, 1 skipped, 1m 35s");
    assert_eq!(check.metric.as_deref(), Some("3/5"));
    assert_eq!(check.findings.len(), 2);
    assert_eq!(check.findings[0].title, "api::broken");
    assert_eq!(check.findings[0].severity.as_deref(), Some("failed"));
    assert_eq!(
        check.findings[0].location.as_deref(),
        Some("tests/api.rs:12")
    );
    assert_eq!(
        check.findings[0].extra.as_deref(),
        Some("assertion failed: left == right")
    );
    // No location of its own: the suite it ran in.
    assert_eq!(check.findings[1].location.as_deref(), Some("Checkout"));
    assert_eq!(check.findings[1].extra, None);
}

#[quench_test::test]
fn a_clean_test_report_passes_and_shows_short_durations() {
    let report = r#"{"duration_ms": 420, "summary": {"total": 2, "passed": 2, "failed": 0, "skipped": 0}, "suites": []}"#;
    let check = tests_check("t", report).expect("a report");
    assert!(check.passed && check.findings.is_empty());
    assert_eq!(check.headline, "2 passed, 0 failed, 0 skipped, 420 ms");

    let report = r#"{"duration_ms": 2500, "summary": {"total": 0}, "suites": []}"#;
    assert!(
        tests_check("t", report)
            .expect("a report")
            .headline
            .ends_with("2.5 s")
    );
}

#[quench_test::test]
fn something_that_is_not_a_test_report_is_not_one() {
    assert!(tests_check("t", "not json").is_none());
    assert!(tests_check("t", "{}").is_none());
    assert!(tests_check("t", r#"{"summary": {}}"#).is_none());
    assert_eq!(CheckKind::from_slug("tests"), Some(CheckKind::Tests));
    assert_eq!(CheckKind::Tests.slug(), "tests");
    assert_eq!(CheckKind::Tests.label(), "ui_scan_tests_title");
}

/// What the security gate writes: a `quench-test` report whose cases are stages and findings.
const SECURITY_REPORT: &str = r#"{
  "duration_ms": 2701000,
  "summary": {"total": 4, "passed": 2, "failed": 1, "skipped": 1},
  "suites": [
    {"name": "supply_chain", "cases": [
      {"name": "cargo_audit", "status": "failed", "message": "RUSTSEC-2026-0001: crate foo 1.2.3 is vulnerable"},
      {"name": "cargo_deny", "status": "passed", "duration_ms": 2000}
    ]},
    {"name": "fuzz", "cases": [
      {"name": "request_head", "status": "passed", "duration_ms": 600000},
      {"name": "ws_frame", "status": "skipped"}
    ]}
  ]
}"#;

#[quench_test::test]
fn a_security_report_is_a_check_of_its_own_with_each_failed_stage_a_finding() {
    let check = security_check("nightly/security", SECURITY_REPORT).expect("a report");

    assert_eq!(check.kind, CheckKind::Security);
    assert!(!check.passed);
    assert_eq!(check.headline, "2 passed, 1 failed, 1 skipped, 45m 1s");
    assert_eq!(check.metric.as_deref(), Some("2/4"));
    assert_eq!(check.findings.len(), 1);
    assert_eq!(check.findings[0].title, "cargo_audit");
    assert_eq!(check.findings[0].location.as_deref(), Some("supply_chain"));
    assert!(check.findings[0].extra.as_deref().unwrap().contains("RUSTSEC-2026-0001"));
}

#[quench_test::test]
fn security_has_its_own_slug_label_and_is_not_the_tests_check() {
    assert_eq!(CheckKind::from_slug("security"), Some(CheckKind::Security));
    assert_eq!(CheckKind::Security.slug(), "security");
    assert_eq!(CheckKind::Security.label(), "ui_scan_security_title");
    assert_ne!(CheckKind::Security, CheckKind::Tests);
    // The same JSON read as tests keeps the tests kind: the artifact name decides, not the content.
    assert_eq!(tests_check("t", SECURITY_REPORT).unwrap().kind, CheckKind::Tests);
}
