//! Unit tests for `routers/ui/pages/home.rs`.
//!
//! The front page's own additions: a chip per check kind, coloured by what
//! the most recent run found, and the manual-run button beside it. What is
//! worth pinning down is the mapping from `CheckResult` to a colour class -
//! everything else is the same table-row plumbing `runs.rs` already covers.

use chrono::Utc;
use conveyor_service::domain::{Provider, Repo};
use conveyor_service::routers::ui::pages::home::{check_chips, chip, run_button};
use conveyor_service::scan::{CheckKind, CheckResult, Finding, ScanSummary};

fn repo(enabled: bool) -> Repo {
    Repo {
        id: "repo-1".to_string(),
        provider: Provider::GitHub,
        owner: "lorehaven".to_string(),
        name: "palantir".to_string(),
        clone_url: "https://github.com/lorehaven/palantir.git".to_string(),
        default_branch: "main".to_string(),
        registered_by: "admin".to_string(),
        enabled,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn check(passed: bool, findings: usize) -> CheckResult {
    CheckResult {
        kind: CheckKind::Lint,
        job_name: "quality/checks".to_string(),
        passed,
        headline: String::new(),
        findings: (0..findings)
            .map(|_| Finding {
                title: "something".to_string(),
                ..Finding::default()
            })
            .collect(),
    }
}

#[test]
fn a_check_that_never_ran_is_a_dashed_chip() {
    let html = chip(CheckKind::Lint, None).render();
    assert!(html.contains("chip-none"));
    assert!(html.contains("L -"));
}

#[test]
fn a_clean_check_is_green_even_with_no_findings_to_count() {
    let html = chip(CheckKind::Machete, Some(&check(true, 0))).render();
    assert!(html.contains("chip-clean"));
    assert!(html.contains("M 0"));
}

#[test]
fn a_passing_check_with_findings_is_amber_not_red() {
    // Lint can pass with warnings - that is not the same as failing, and the
    // chip's colour is the whole point of it existing over a bare number.
    let html = chip(CheckKind::Lint, Some(&check(true, 3))).render();
    assert!(html.contains("chip-warning"));
    assert!(html.contains("L 3"));
    assert!(!html.contains("chip-danger"));
}

#[test]
fn a_failed_check_is_red() {
    let html = chip(CheckKind::Audit, Some(&check(false, 1))).render();
    assert!(html.contains("chip-danger"));
    assert!(html.contains("A 1"));
}

#[test]
fn three_chips_come_out_in_a_fixed_order_regardless_of_which_checks_ran() {
    let summary = ScanSummary {
        run: None,
        lint: Some(check(true, 0)),
        machete: None,
        audit: Some(check(false, 2)),
    };

    let html = check_chips(Some(&summary)).render();
    let lint_at = html.find("L 0").expect("lint chip");
    let machete_at = html.find("M -").expect("machete chip, unrun");
    let audit_at = html.find("A 2").expect("audit chip");

    assert!(lint_at < machete_at && machete_at < audit_at, "got: {html}");
}

#[test]
fn a_repo_with_no_summary_at_all_shows_three_unrun_chips() {
    let html = check_chips(None).render();
    assert!(html.contains("L -"));
    assert!(html.contains("M -"));
    assert!(html.contains("A -"));
}

#[test]
fn an_enabled_repo_gets_a_run_button_that_posts_to_its_own_id() {
    let html = run_button(&repo(true)).render();
    assert!(html.contains("<button"));
    assert!(html.contains(r#"hx-post"#));
    assert!(html.contains("repos/repo-1/run"));
    assert!(html.contains("hx-target=\"#home-state\""));
}

#[test]
fn a_disabled_repo_gets_no_run_button() {
    // The API rejects a manual run on a disabled repository with a 409; a
    // button that only ever fails is worse than no button at all.
    let html = run_button(&repo(false)).render();
    assert!(!html.contains("<button"));
    assert!(!html.contains("hx-post"));
}
