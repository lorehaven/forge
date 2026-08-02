//! Unit tests for `routers/ui/pages/home.rs`.
//!
//! The front page's own additions: a chip per check kind, coloured by what
//! the most recent run found, and the manual-run button beside it. What is
//! worth pinning down is the mapping from `CheckResult` to a colour class -
//! everything else is the same table-row plumbing `runs.rs` already covers.

use chrono::Utc;
use conveyor_service::domain::{Project, Provider, Repo};
use conveyor_service::routers::ui::pages::home::{
    check_chips, chip, project_tree_panel, run_button,
};
use conveyor_service::scan::{CheckKind, CheckResult, Finding, ScanSummary};
use std::collections::HashMap;

fn repo(enabled: bool) -> Repo {
    Repo {
        id: "repo-1".to_string(),
        provider: Provider::GitHub,
        owner: "lorehaven".to_string(),
        name: "palantir".to_string(),
        clone_url: "https://github.com/lorehaven/palantir.git".to_string(),
        default_branch: "main".to_string(),
        registered_by: "admin".to_string(),
        project_id: "project-1".to_string(),
        enabled,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn project(id: &str, name: &str, parent_id: Option<&str>) -> Project {
    Project {
        id: id.to_string(),
        name: name.to_string(),
        parent_id: parent_id.map(str::to_string),
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

#[test]
fn a_nested_project_renders_as_a_nested_details_element() {
    let root = project("root-1", "root-group", None);
    let child = project("child-1", "child-project", Some("root-1"));
    let mut leaf_repo = repo(true);
    leaf_repo.project_id = "child-1".to_string();

    let html = project_tree_panel(&[root, child], &[leaf_repo], &HashMap::new()).render();

    // One disclosure per node - a container is not a different element from a
    // leaf, it just has different children.
    assert_eq!(html.matches(r#"class="project-node""#).count(), 2);
    // The container comes before what is nested inside it, textually as well
    // as structurally.
    let root_at = html.find("root-group").expect("root node");
    let child_at = html.find("child-project").expect("child node");
    let repo_at = html.find("lorehaven/palantir").expect("the repo row");
    assert!(root_at < child_at && child_at < repo_at, "got: {html}");
}

#[test]
fn a_repo_appears_under_its_own_project_and_nowhere_else() {
    let a = project("a", "project-a", None);
    let b = project("b", "project-b", None);
    let mut repo_a = repo(true);
    repo_a.id = "repo-a".to_string();
    repo_a.name = "in-a".to_string();
    repo_a.project_id = "a".to_string();
    let mut repo_b = repo(true);
    repo_b.id = "repo-b".to_string();
    repo_b.name = "in-b".to_string();
    repo_b.project_id = "b".to_string();

    let html = project_tree_panel(&[a, b], &[repo_a, repo_b], &HashMap::new()).render();

    // Split on the second root's own summary: everything before it is
    // project-a's whole subtree, so repo-b cannot have leaked into it.
    let (a_section, b_section) = html.split_once("project-b").expect("project-b's node");
    assert!(a_section.contains("lorehaven/in-a"));
    assert!(!a_section.contains("lorehaven/in-b"));
    assert!(b_section.contains("lorehaven/in-b"));
}

#[test]
fn a_project_with_no_repo_and_no_children_is_still_a_valid_leaf() {
    let empty = project("solo", "just-a-group", None);
    let html = project_tree_panel(&[empty], &[], &HashMap::new()).render();

    assert!(html.contains("just-a-group"));
    assert!(!html.contains("<table"));
}

#[test]
fn no_projects_at_all_shows_the_empty_message() {
    let html = project_tree_panel(&[], &[], &HashMap::new()).render();
    assert!(html.contains("ui_repos_empty"));
    assert!(!html.contains("project-node"));
}
