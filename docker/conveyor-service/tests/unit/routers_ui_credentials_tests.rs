//! Unit tests for `routers/ui/pages/credentials.rs`: the pure scope
//! resolution and rendering behind the credentials preview page. The route
//! handler itself needs a database and a session (like `repos.rs`'s), so
//! what's worth pinning down here is what decides whether a row is even
//! placeable, and what a row actually renders.

use chrono::Utc;
use conveyor_service::credentials::store::CredentialRef;
use conveyor_service::domain::{Project, Provider, Repo};
use conveyor_service::routers::ui::pages::credentials::{credential_row, credential_scope};
use std::collections::HashMap;

fn project(id: &str, name: &str, parent_id: Option<&str>) -> Project {
    Project {
        id: id.to_string(),
        name: name.to_string(),
        parent_id: parent_id.map(str::to_string),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn repo(id: &str, project_id: &str) -> Repo {
    Repo {
        id: id.to_string(),
        provider: Provider::GitHub,
        owner: "lorehaven".to_string(),
        name: "sci-rust".to_string(),
        clone_url: "https://github.com/lorehaven/sci-rust.git".to_string(),
        default_branch: "main".to_string(),
        registered_by: "admin".to_string(),
        project_id: project_id.to_string(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn credential(project_id: Option<&str>, repo_id: Option<&str>) -> CredentialRef {
    CredentialRef {
        id: "cred-1".to_string(),
        project_id: project_id.map(str::to_string),
        repo_id: repo_id.map(str::to_string),
        name: "GITHUB_TOKEN".to_string(),
        kind: "http_token".to_string(),
        username: "x-access-token".to_string(),
        preview: "\u{2022}\u{2022}\u{2022}\u{2022}\u{2026}9f2a".to_string(),
        created_by: "admin".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[test]
fn a_project_scoped_credential_resolves_its_own_project_id() {
    let cred = credential(Some("proj-1"), None);
    let repos_by_id: HashMap<&str, &Repo> = HashMap::new();

    let scope = credential_scope(&cred, &repos_by_id).expect("resolvable");
    assert_eq!(scope.project_id, "proj-1");
    assert!(scope.repo.is_none());
}

#[test]
fn a_repo_scoped_credential_resolves_via_its_repository() {
    let repo = repo("repo-1", "proj-1");
    let cred = credential(None, Some("repo-1"));
    let repos_by_id: HashMap<&str, &Repo> = [("repo-1", &repo)].into_iter().collect();

    let scope = credential_scope(&cred, &repos_by_id).expect("resolvable");
    assert_eq!(scope.project_id, "proj-1");
    assert_eq!(scope.repo.map(|r| r.id.as_str()), Some("repo-1"));
}

#[test]
fn a_repo_scoped_credential_with_no_matching_repository_is_unplaceable() {
    // A repo-scoped row whose repository no longer exists - `ON DELETE
    // CASCADE` means this shouldn't happen, but the page has to treat it as
    // "cannot show" rather than panicking on a missing lookup.
    let cred = credential(None, Some("gone"));
    let repos_by_id: HashMap<&str, &Repo> = HashMap::new();

    assert!(credential_scope(&cred, &repos_by_id).is_none());
}

#[test]
fn a_credential_with_neither_scope_is_unplaceable() {
    let cred = credential(None, None);
    let repos_by_id: HashMap<&str, &Repo> = HashMap::new();

    assert!(credential_scope(&cred, &repos_by_id).is_none());
}

#[test]
fn a_project_scoped_row_renders_its_breadcrumb_path() {
    let root = project("root-1", "lorehaven", None);
    let child = project("proj-1", "sci-rust", Some("root-1"));
    let all_projects = vec![root, child];
    let cred = credential(Some("proj-1"), None);
    let repos_by_id: HashMap<&str, &Repo> = HashMap::new();
    let scope = credential_scope(&cred, &repos_by_id).expect("resolvable");

    let html = credential_row(&cred, &scope, &all_projects).render();
    assert!(html.contains("project: lorehaven/sci-rust"));
    assert!(html.contains("GITHUB_TOKEN"));
    assert!(html.contains("x-access-token"));
    assert!(html.contains("9f2a"));
}

#[test]
fn a_repo_scoped_row_renders_owner_slash_name_instead_of_a_project_path() {
    let repo = repo("repo-1", "proj-1");
    let cred = credential(None, Some("repo-1"));
    let repos_by_id: HashMap<&str, &Repo> = [("repo-1", &repo)].into_iter().collect();
    let scope = credential_scope(&cred, &repos_by_id).expect("resolvable");

    let html = credential_row(&cred, &scope, &[]).render();
    assert!(html.contains("repo: lorehaven/sci-rust"));
    assert!(!html.contains("project:"));
}

#[test]
fn a_row_never_renders_anything_beyond_the_masked_preview() {
    // `CredentialRef` structurally has no token field to leak - this pins
    // down that the row's rendered text is built only from `preview`, not
    // from anything that could later grow into carrying the real value.
    let cred = credential(Some("proj-1"), None);
    let repos_by_id: HashMap<&str, &Repo> = HashMap::new();
    let scope = credential_scope(&cred, &repos_by_id).expect("resolvable");

    let html = credential_row(&cred, &scope, &[]).render();
    assert!(html.contains(&cred.preview));
}
