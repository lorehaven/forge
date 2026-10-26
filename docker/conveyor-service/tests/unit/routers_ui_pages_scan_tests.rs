use conveyor_service::domain::{Provider, Repo, Run, Status, Trigger};
use conveyor_service::routers::ui::pages::scan::{card, detail, overview, overview_body};
use conveyor_service::scan::{CheckKind, CheckResult, Finding, ScanSummary};
use quench_core::time::Time;

fn repo() -> Repo {
    Repo {
        id: "repo-1".to_string(),
        provider: Provider::GitHub,
        owner: "lorehaven".to_string(),
        name: "palantir".to_string(),
        clone_url: "https://github.com/lorehaven/palantir.git".to_string(),
        default_branch: "main".to_string(),
        registered_by: "admin".to_string(),
        project_id: "project-1".to_string(),
        enabled: true,
        created_at: Time::now(),
        updated_at: Time::now(),
    }
}

fn run() -> Run {
    Run {
        id: "run-1".to_string(),
        repo_id: "repo-1".to_string(),
        trigger: Trigger::Push,
        git_ref: "refs/heads/main".to_string(),
        sha: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
        message: Some("a commit".to_string()),
        delivery_id: None,
        status: Status::Success,
        queued_at: Time::now(),
        started_at: Some(Time::now()),
        finished_at: Some(Time::now()),
        claimed_by: None,
        claimed_at: None,
        attempt: 0,
        error: None,
        resumed_from: None,
        stages: None,
    }
}

#[quench_test::test]
fn renders_no_runs_yet() {
    let html = overview_body(&repo(), &ScanSummary::default()).render();
    assert!(html.contains("ui_scan_no_runs"));
}

#[quench_test::test]
fn renders_no_checks_configured() {
    let summary = ScanSummary {
        run: Some(run()),
        ..ScanSummary::default()
    };
    let html = overview_body(&repo(), &summary).render();
    assert!(html.contains("ui_scan_no_checks"));
    assert!(html.contains("deadbee"));
}

#[quench_test::test]
fn renders_cards_with_counts_and_links() {
    let summary = ScanSummary {
        run: Some(run()),
        lint: Some(CheckResult {
            kind: CheckKind::Lint,
            job_name: "quality/checks".to_string(),
            passed: false,
            headline: "1 warning".to_string(),
            findings: vec![Finding {
                title: "unused variable: x".to_string(),
                severity: Some("warning".to_string()),
                ..Finding::default()
            }],
            metric: None,
            report_uri: None,
        }),
        machete: None,
        audit: None,
        coverage: None,
        tests: None,
        security: None,
        origins: vec![],
    };

    let html = overview(&repo(), &summary).render();

    assert!(html.contains("lorehaven/palantir"));
    assert!(html.contains("ui_scan_lint_title"));
    assert!(html.contains(">1<"));
    assert!(html.contains("/ui/repos/lorehaven/palantir/scan/lint"));
    assert!(html.contains("status-failed"));
    // machete/audit/coverage weren't configured - no cards for them at all.
    assert!(!html.contains("ui_scan_machete_title"));
    assert!(!html.contains("ui_scan_audit_title"));
    assert!(!html.contains("ui_scan_coverage_title"));
}

#[quench_test::test]
fn a_coverage_card_shows_its_metric_not_the_capped_finding_count() {
    let check = CheckResult {
        kind: CheckKind::Coverage,
        job_name: "test/coverage".to_string(),
        passed: true,
        headline: "22.10% line coverage".to_string(),
        // Capped at 50 the same way a real, mostly-uncovered workspace
        // would be - the card must still show the percentage, not "50".
        findings: (0..50)
            .map(|_| Finding {
                title: "some/file.rs".to_string(),
                ..Finding::default()
            })
            .collect(),
        metric: Some("22%".to_string()),
        report_uri: None,
    };

    let html = card(&repo(), &check).render();

    assert!(html.contains(">22%<"));
    assert!(!html.contains(">50<"));
}

#[quench_test::test]
fn renders_finding_detail_fields() {
    let check = CheckResult {
        kind: CheckKind::Audit,
        job_name: "quality/checks".to_string(),
        passed: false,
        headline: "1 finding".to_string(),
        findings: vec![Finding {
            title: "Marvin Attack".to_string(),
            id: Some("RUSTSEC-2023-0071".to_string()),
            severity: Some("5.9 (medium)".to_string()),
            date: Some("2023-11-22".to_string()),
            location: Some("rsa 0.9.10".to_string()),
            extra: Some("Solution: No fixed upgrade is available!".to_string()),
        }],
        metric: None,
        report_uri: None,
    };

    let html = detail(&repo(), CheckKind::Audit, &check).render();

    assert!(html.contains("Marvin Attack"));
    assert!(html.contains("RUSTSEC-2023-0071"));
    assert!(html.contains("5.9 (medium)"));
    assert!(html.contains("2023-11-22"));
    assert!(html.contains("rsa 0.9.10"));
    assert!(html.contains("No fixed upgrade is available!"));
}

#[quench_test::test]
fn renders_clean_detail_page() {
    let check = CheckResult {
        kind: CheckKind::Machete,
        job_name: "quality/checks".to_string(),
        passed: true,
        headline: "clean".to_string(),
        findings: vec![],
        metric: None,
        report_uri: None,
    };

    let html = detail(&repo(), CheckKind::Machete, &check).render();
    assert!(html.contains("ui_scan_clean"));
}

#[quench_test::test]
fn a_tests_card_shows_passed_over_total_and_links_to_its_page() {
    let summary = ScanSummary {
        run: Some(run()),
        tests: Some(CheckResult {
            kind: CheckKind::Tests,
            job_name: "test/unit".to_string(),
            passed: true,
            headline: "9 passed, 0 failed, 1 skipped".to_string(),
            findings: vec![],
            metric: Some("9/10".to_string()),
            report_uri: None,
        }),
        ..ScanSummary::default()
    };

    let html = overview(&repo(), &summary).render();

    assert!(html.contains("ui_scan_tests_title"));
    assert!(html.contains(">9/10<"));
    assert!(html.contains("/ui/repos/lorehaven/palantir/scan/tests"));
    assert!(html.contains("status-success"));
}

#[quench_test::test]
fn the_tests_page_shows_the_report_in_place_with_buttons_to_open_and_save_it() {
    let mut check = CheckResult {
        kind: CheckKind::Tests,
        job_name: "test/unit".to_string(),
        passed: true,
        headline: "1 passed, 0 failed, 0 skipped".to_string(),
        findings: vec![],
        metric: Some("1/1".to_string()),
        report_uri: Some("http://warehouse/api/v1/files/artifacts/file?path=x".to_string()),
    };

    let html = detail(&repo(), CheckKind::Tests, &check).render();
    // In a sandboxed frame served by conveyor itself, not a link out to warehouse.
    assert!(html.contains("<iframe"));
    assert!(html.contains(r#"sandbox="allow-scripts""#));
    assert!(
        html.contains(r#"src="/ui/repos/lorehaven/palantir/scan/tests/report""#),
        "{html}"
    );
    assert!(html.contains("scan/tests/report?download=1"));
    assert!(
        html.contains("ui_scan_tests_report_open")
            && html.contains("ui_scan_tests_report_download")
    );
    assert!(!html.contains("warehouse/api"));

    check.report_uri = None;
    assert!(
        !detail(&repo(), CheckKind::Tests, &check)
            .render()
            .contains("<iframe")
    );
}

#[quench_test::test]
fn a_security_card_sits_beside_the_others_and_its_page_embeds_its_own_report() {
    let security = CheckResult {
        kind: CheckKind::Security,
        job_name: "security/nightly, 2026-10-26 00:00 UTC".to_string(),
        passed: false,
        headline: "40 passed, 2 failed, 0 skipped, 41m 3s".to_string(),
        findings: vec![],
        metric: Some("40/42".to_string()),
        report_uri: Some("http://warehouse/api/v1/files/artifacts/file?path=s".to_string()),
    };
    let summary = ScanSummary {
        run: Some(run()),
        security: Some(security.clone()),
        ..ScanSummary::default()
    };

    let html = overview(&repo(), &summary).render();
    assert!(html.contains("ui_scan_security_title"));
    assert!(html.contains(">40/42<"));
    assert!(html.contains("/ui/repos/lorehaven/palantir/scan/security"));
    assert!(html.contains("status-failed"));

    // The page embeds the *security* report - not the unit tests' - and offers it for download.
    let page = detail(&repo(), CheckKind::Security, &security).render();
    assert!(
        page.contains(r#"src="/ui/repos/lorehaven/palantir/scan/security/report""#),
        "{page}"
    );
    assert!(page.contains("scan/security/report?download=1"));
    assert!(!page.contains("scan/tests/report"));
}

#[quench_test::test]
fn each_card_says_which_run_it_came_from() {
    let check = CheckResult {
        kind: CheckKind::Security,
        job_name: "security/nightly".to_string(),
        passed: true,
        headline: "all clear".to_string(),
        findings: vec![],
        metric: Some("4/4".to_string()),
        report_uri: None,
    };
    let mut nightly = run();
    nightly.trigger = conveyor_service::domain::Trigger::Schedule;

    let html =
        conveyor_service::routers::ui::pages::scan::card_from(&repo(), &check, Some(&nightly))
            .render();
    assert!(html.contains("scan-card-origin"), "{html}");
    assert!(html.contains(nightly.short_sha()), "{html}");
    assert!(html.contains("schedule"), "{html}");

    // Without a known origin there is no line to show, and the card is the one it always was.
    let plain = card(&repo(), &check).render();
    assert!(!plain.contains("scan-card-origin"));
}
