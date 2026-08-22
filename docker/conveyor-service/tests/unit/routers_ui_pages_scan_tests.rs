use chrono::Utc;
use conveyor_service::domain::{Provider, Repo, Run, Status, Trigger};
use conveyor_service::routers::ui::pages::scan::{card, detail, overview, overview_body};
use conveyor_service::scan::{CheckKind, CheckResult, Finding, ScanSummary};

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
        created_at: Utc::now(),
        updated_at: Utc::now(),
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
        queued_at: Utc::now(),
        started_at: Some(Utc::now()),
        finished_at: Some(Utc::now()),
        claimed_by: None,
        claimed_at: None,
        attempt: 0,
        error: None,
    }
}

#[test]
fn renders_no_runs_yet() {
    let html = overview_body(&repo(), &ScanSummary::default()).render();
    assert!(html.contains("ui_scan_no_runs"));
}

#[test]
fn renders_no_checks_configured() {
    let summary = ScanSummary {
        run: Some(run()),
        ..ScanSummary::default()
    };
    let html = overview_body(&repo(), &summary).render();
    assert!(html.contains("ui_scan_no_checks"));
    assert!(html.contains("deadbee"));
}

#[test]
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
        }),
        machete: None,
        audit: None,
        coverage: None,
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

#[test]
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
    };

    let html = card(&repo(), &check).render();

    assert!(html.contains(">22%<"));
    assert!(!html.contains(">50<"));
}

#[test]
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
    };

    let html = detail(&repo(), CheckKind::Audit, &check).render();

    assert!(html.contains("Marvin Attack"));
    assert!(html.contains("RUSTSEC-2023-0071"));
    assert!(html.contains("5.9 (medium)"));
    assert!(html.contains("2023-11-22"));
    assert!(html.contains("rsa 0.9.10"));
    assert!(html.contains("No fixed upgrade is available!"));
}

#[test]
fn renders_clean_detail_page() {
    let check = CheckResult {
        kind: CheckKind::Machete,
        job_name: "quality/checks".to_string(),
        passed: true,
        headline: "clean".to_string(),
        findings: vec![],
        metric: None,
    };

    let html = detail(&repo(), CheckKind::Machete, &check).render();
    assert!(html.contains("ui_scan_clean"));
}
