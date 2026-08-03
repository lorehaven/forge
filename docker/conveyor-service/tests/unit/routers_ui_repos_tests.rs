//! Unit tests for `routers/ui/pages/repos.rs`: the pure rendering and
//! lookup helpers behind the add/edit/delete repository pages. The route
//! handlers themselves need a database and a session, so they are left to
//! the BDD suite; what is worth pinning down here is the logic that decides
//! what a visitor sees - the project-path lookup, which controls are shown
//! to someone without a write grant, and the error-key allowlist that keeps
//! a crafted `?err=` query from injecting an arbitrary translation key.

use chrono::Utc;
use conveyor_service::domain::{Project, Provider, Repo};
use conveyor_service::routers::ui::pages::repos::{
    Notice, create_panel, edit_fields, known_error_key, notice_banner, project_path, repo_row,
};

fn project(id: &str, name: &str, parent_id: Option<&str>) -> Project {
    Project {
        id: id.to_string(),
        name: name.to_string(),
        parent_id: parent_id.map(str::to_string),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn repo() -> Repo {
    Repo {
        id: "repo-1".to_string(),
        provider: Provider::GitHub,
        owner: "lorehaven".to_string(),
        name: "palantir".to_string(),
        clone_url: "https://github.com/lorehaven/palantir.git".to_string(),
        default_branch: "main".to_string(),
        registered_by: "admin".to_string(),
        project_id: "child-1".to_string(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[test]
fn project_path_walks_up_to_the_root() {
    let root = project("root-1", "root-group", None);
    let child = project("child-1", "child-project", Some("root-1"));

    assert_eq!(
        project_path("child-1", &[root, child]),
        "root-group/child-project"
    );
}

#[test]
fn project_path_of_an_unknown_id_is_empty() {
    let root = project("root-1", "root-group", None);
    assert_eq!(project_path("missing", std::slice::from_ref(&root)), "");
}

#[test]
fn a_repo_row_only_gets_an_edit_link_when_the_project_is_writable() {
    let html = repo_row(&repo(), "root-group/child-project", false).render();
    assert!(!html.contains("repos-edit"));
    assert!(html.contains("lorehaven/palantir"));

    let editable_html = repo_row(&repo(), "root-group/child-project", true).render();
    assert!(editable_html.contains("repos-edit"));
    assert!(editable_html.contains("/repos/lorehaven/palantir/edit"));
}

#[test]
fn the_create_panel_is_omitted_with_nowhere_writable_to_register_into() {
    let all_projects = vec![project("root-1", "root-group", None)];
    assert!(create_panel(&[], &all_projects).is_none());
}

#[test]
fn the_create_panel_offers_every_writable_project() {
    let root = project("root-1", "root-group", None);
    let child = project("child-1", "child-project", Some("root-1"));
    let all_projects = vec![root.clone(), child.clone()];

    let html = create_panel(&[&root, &child], &all_projects)
        .expect("at least one writable project")
        .render();

    assert!(html.contains("root-group"));
    assert!(html.contains("root-group/child-project"));
}

#[test]
fn edit_fields_are_disabled_and_collapsed_to_the_current_project_without_write_access() {
    let repo = repo();
    let child = project("child-1", "child-project", None);
    let all_projects = vec![child];

    let html = edit_fields(&repo, &all_projects, &[], true).render();

    assert!(html.contains(r#"disabled="disabled""#));
    // The repo's own project still shows up, even though it is not among the
    // (empty) writable set - otherwise the select would render with nothing
    // in it at all.
    assert!(html.contains("child-project"));
}

#[test]
fn edit_fields_offer_only_writable_projects_when_editable() {
    let repo = repo();
    let writable = project("child-1", "child-project", None);
    let other = project("other-1", "other-project", None);
    let all_projects = vec![writable.clone(), other];

    let html = edit_fields(&repo, &all_projects, &[&writable], false).render();

    assert!(!html.contains(r#"disabled="disabled""#));
    assert!(html.contains("child-project"));
    assert!(!html.contains("other-project"));
}

#[test]
fn only_known_error_slugs_translate_to_a_notice() {
    assert_eq!(known_error_key("forbidden"), Some("ui_repos_err_forbidden"));
    // A hand-crafted `?err=` cannot smuggle an arbitrary translation key onto
    // the page - anything not on the allowlist renders no banner at all.
    assert_eq!(known_error_key("<script>alert(1)</script>"), None);
}

#[test]
fn an_unknown_ok_value_renders_no_banner() {
    let notice = Notice {
        err: None,
        ok: Some("something-unexpected".to_string()),
    };
    assert!(notice_banner(&notice).is_none());
}

#[test]
fn a_known_ok_value_renders_its_own_key() {
    let notice = Notice {
        err: None,
        ok: Some("deleted".to_string()),
    };
    let html = notice_banner(&notice).expect("a banner").render();
    assert!(html.contains("ui_repos_ok_deleted"));
}
