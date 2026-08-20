//! Unit tests for `routers/ui/pages/home.rs`.
//!
//! The front page's own additions: a chip per check kind, coloured by what
//! the most recent run found, and the manual-run button beside it. What is
//! worth pinning down is the mapping from `CheckResult` to a colour class -
//! everything else is the same table-row plumbing `runs.rs` already covers.

use chrono::Utc;
use conveyor_service::domain::{Project, Provider, Repo};
use conveyor_service::routers::ui::pages::home::{
    check_chips, chip, project_tree_panel, project_tree_panel_scoped, run_button,
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
fn four_chips_come_out_in_a_fixed_order_regardless_of_which_checks_ran() {
    let summary = ScanSummary {
        run: None,
        lint: Some(check(true, 0)),
        machete: None,
        audit: Some(check(false, 2)),
        coverage: None,
    };

    let html = check_chips(Some(&summary)).render();
    let lint_at = html.find("L 0").expect("lint chip");
    let machete_at = html.find("M -").expect("machete chip, unrun");
    let audit_at = html.find("A 2").expect("audit chip");
    let coverage_at = html.find("C -").expect("coverage chip, unrun");

    assert!(
        lint_at < machete_at && machete_at < audit_at && audit_at < coverage_at,
        "got: {html}"
    );
}

#[test]
fn a_repo_with_no_summary_at_all_shows_four_unrun_chips() {
    let html = check_chips(None).render();
    assert!(html.contains("L -"));
    assert!(html.contains("M -"));
    assert!(html.contains("A -"));
    assert!(html.contains("C -"));
}

#[test]
fn an_enabled_repo_gets_a_run_button_that_posts_to_its_own_id() {
    let html = run_button(&repo(true), None).render();
    assert!(html.contains("<button"));
    assert!(html.contains(r#"hx-post"#));
    assert!(html.contains("repos/repo-1/run"));
    assert!(html.contains("hx-target=\"#home-state\""));
}

#[test]
fn a_disabled_repo_gets_no_run_button() {
    // The API rejects a manual run on a disabled repository with a 409; a
    // button that only ever fails is worse than no button at all.
    let html = run_button(&repo(false), None).render();
    assert!(!html.contains("<button"));
    assert!(!html.contains("hx-post"));
}

#[test]
fn a_run_button_on_a_scoped_page_carries_the_scope_along() {
    // Clicking "run now" from a project's own branch of the page has to come
    // back to that same branch, not silently jump to the unscoped front page.
    let html = run_button(&repo(true), Some("project-9")).render();
    assert!(html.contains("repos/repo-1/run?project=project-9"));
}

#[test]
fn a_leaf_child_folds_into_its_parents_table_with_no_details_of_its_own() {
    let root = project("root-1", "root-group", None);
    let child = project("child-1", "child-project", Some("root-1"));
    let mut leaf_repo = repo(true);
    leaf_repo.project_id = "child-1".to_string();

    let html = project_tree_panel(&[root, child], &[leaf_repo], &HashMap::new()).render();

    // `child-project` has no children of its own, so it is a leaf: expanding
    // it would only ever reveal the one repo it already is. Only the root -
    // which has something nested under it - gets a disclosure.
    assert_eq!(html.matches(r#"class="project-node""#).count(), 1);
    assert!(html.contains("root-group"));
    // The leaf's own name is not shown at all - its repo row stands in for it.
    assert!(!html.contains("child-project"));
    assert!(html.contains("palantir"));
}

#[test]
fn a_container_with_its_own_children_still_gets_a_nested_details() {
    let root = project("root-1", "root-group", None);
    let mid = project("mid-1", "mid-group", Some("root-1"));
    let leaf = project("leaf-1", "leaf-project", Some("mid-1"));
    let mut leaf_repo = repo(true);
    leaf_repo.project_id = "leaf-1".to_string();

    let html = project_tree_panel(&[root, mid, leaf], &[leaf_repo], &HashMap::new()).render();

    // `mid-group` has a child (`leaf-project`) even though it holds no repo
    // itself, so it is a container and keeps its own disclosure - only
    // `leaf-project`, which has nothing further nested under it, folds away.
    assert_eq!(html.matches(r#"class="project-node""#).count(), 2);
    let root_at = html.find("root-group").expect("root node");
    let mid_at = html.find("mid-group").expect("mid node");
    let repo_at = html.find("palantir").expect("the repo row");
    assert!(root_at < mid_at && mid_at < repo_at, "got: {html}");
    assert!(!html.contains("leaf-project"));
}

#[test]
fn a_repo_appears_under_its_own_branch_and_nowhere_else() {
    let a = project("a", "project-a", None);
    let a_child = project("a-child", "a-child", Some("a"));
    let b = project("b", "project-b", None);
    let b_child = project("b-child", "b-child", Some("b"));
    let mut repo_a = repo(true);
    repo_a.id = "repo-a".to_string();
    repo_a.name = "in-a".to_string();
    repo_a.project_id = "a-child".to_string();
    let mut repo_b = repo(true);
    repo_b.id = "repo-b".to_string();
    repo_b.name = "in-b".to_string();
    repo_b.project_id = "b-child".to_string();

    let html = project_tree_panel(
        &[a, a_child, b, b_child],
        &[repo_a, repo_b],
        &HashMap::new(),
    )
    .render();

    // Split on the second root's own summary: everything before it is
    // project-a's whole subtree, so repo-b cannot have leaked into it.
    let (a_section, b_section) = html.split_once("project-b").expect("project-b's node");
    assert!(a_section.contains("in-a"));
    assert!(!a_section.contains("in-b"));
    assert!(b_section.contains("in-b"));
}

#[test]
fn two_leaf_repos_under_the_same_parent_share_one_table() {
    let root = project("root-1", "root-group", None);
    let a = project("a", "child-a", Some("root-1"));
    let b = project("b", "child-b", Some("root-1"));
    let mut repo_a = repo(true);
    repo_a.id = "repo-a".to_string();
    repo_a.name = "in-a".to_string();
    repo_a.project_id = "a".to_string();
    let mut repo_b = repo(true);
    repo_b.id = "repo-b".to_string();
    repo_b.name = "in-b".to_string();
    repo_b.project_id = "b".to_string();

    let html = project_tree_panel(&[root, a, b], &[repo_a, repo_b], &HashMap::new()).render();

    // Neither leaf child needs its own details, and there is exactly one
    // repository table for the two of them to share - not one apiece.
    assert_eq!(html.matches(r#"class="project-node""#).count(), 1);
    assert_eq!(html.matches("<table").count(), 1);
    assert!(html.contains("in-a"));
    assert!(html.contains("in-b"));
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

#[test]
fn a_container_node_name_is_a_link_to_its_own_project_page() {
    let root = project("root-1", "root-group", None);
    let child = project("child-1", "child-project", Some("root-1"));
    let mut leaf_repo = repo(true);
    leaf_repo.project_id = "child-1".to_string();

    let html = project_tree_panel(&[root, child], &[leaf_repo], &HashMap::new()).render();

    // Attribute order in the rendered tag is not guaranteed, so each of these
    // is checked independently rather than as one literal fragment.
    assert!(html.contains(r#"class="project-name""#));
    assert!(html.contains(r#"href="/ui/projects/root-1""#));
    assert!(html.contains(">root-group<"));
}

#[test]
fn an_empty_leaf_is_still_a_link_to_its_own_project_page() {
    let empty = project("solo", "just-a-group", None);
    let html = project_tree_panel(&[empty], &[], &HashMap::new()).render();

    assert!(html.contains(r#"href="/ui/projects/solo""#));
    assert!(html.contains("just-a-group"));
}

#[test]
fn scoping_to_a_root_shows_only_that_branch_and_hides_the_root_itself() {
    let root = project("root-1", "root-group", None);
    let child = project("child-1", "child-project", Some("root-1"));
    let sibling_root = project("other-root", "other-group", None);
    let mut repo_a = repo(true);
    repo_a.id = "repo-a".to_string();
    repo_a.name = "in-root".to_string();
    repo_a.project_id = "child-1".to_string();
    let mut repo_b = repo(true);
    repo_b.id = "repo-b".to_string();
    repo_b.name = "in-other".to_string();
    repo_b.project_id = "other-root".to_string();

    let html = project_tree_panel_scoped(
        &[root, child, sibling_root],
        &[repo_a, repo_b],
        &HashMap::new(),
        Some("root-1"),
    )
    .render();

    // The scoped root's own descendants show up...
    assert!(html.contains("in-root"));
    // ...but the root's own name is not repeated (the page header says where
    // we are), and nothing from outside the scope leaks in.
    assert!(!html.contains("root-group"));
    assert!(!html.contains("other-group"));
    assert!(!html.contains("in-other"));
}

#[test]
fn a_root_with_its_own_direct_repo_shows_it_even_with_no_children() {
    let root = project("root-1", "root-group", None);
    let mut direct_repo = repo(true);
    direct_repo.project_id = "root-1".to_string();

    let html = project_tree_panel_scoped(&[root], &[direct_repo], &HashMap::new(), Some("root-1"))
        .render();

    assert!(html.contains("palantir"));
}
