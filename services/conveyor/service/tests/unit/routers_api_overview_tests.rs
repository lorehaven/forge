//! The pure parts of the overview reads: which runs the front page keeps, and which projects and repositories a scope covers.

use conveyor_service::domain::{Project, Provider, Repo, Run, Status, Trigger};
use conveyor_service::routers::api::overview::{
    cap_per_repo, descendant_project_ids, repo_ids_under,
};
use quench_core::time::Time;
use std::collections::HashSet;

fn project(id: &str, parent_id: Option<&str>) -> Project {
    Project {
        id: id.to_string(),
        name: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        created_at: Time::now(),
        updated_at: Time::now(),
    }
}

fn repo(id: &str, project_id: &str) -> Repo {
    Repo {
        id: id.to_string(),
        provider: Provider::GitHub,
        owner: "lorehaven".to_string(),
        name: id.to_string(),
        clone_url: format!("https://github.com/lorehaven/{id}.git"),
        default_branch: "main".to_string(),
        registered_by: "admin".to_string(),
        project_id: project_id.to_string(),
        enabled: true,
        created_at: Time::now(),
        updated_at: Time::now(),
    }
}

fn run(id: &str, repo_id: &str) -> Run {
    Run {
        id: id.to_string(),
        repo_id: repo_id.to_string(),
        trigger: Trigger::Push,
        git_ref: "refs/heads/main".to_string(),
        sha: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
        message: None,
        delivery_id: None,
        status: Status::Success,
        queued_at: Time::now(),
        started_at: None,
        finished_at: None,
        claimed_by: None,
        claimed_at: None,
        attempt: 0,
        error: None,
        resumed_from: None,
        stages: None,
    }
}

#[quench_test::test]
fn cap_per_repo_keeps_only_the_newest_from_each_repository() {
    // Newest first, as `queue::list_runs_page` already returns them.
    let runs = vec![
        run("r3", "repo-a"),
        run("r2", "repo-a"),
        run("r1", "repo-b"),
    ];
    let capped = cap_per_repo(&runs, 5, 1);
    assert_eq!(capped.len(), 2);
    assert_eq!(capped[0].id, "r3");
    assert_eq!(capped[1].id, "r1");
}

#[quench_test::test]
fn cap_per_repo_stops_at_the_total_even_with_room_left_per_repo() {
    let runs = vec![run("r1", "a"), run("r2", "b"), run("r3", "c")];
    assert_eq!(cap_per_repo(&runs, 2, 5).len(), 2);
}

#[quench_test::test]
fn cap_per_repo_can_keep_more_than_one_per_repository() {
    let runs = vec![run("r3", "a"), run("r2", "a"), run("r1", "a")];
    let capped = cap_per_repo(&runs, 5, 2);
    assert_eq!(capped.len(), 2);
    assert_eq!(capped[0].id, "r3");
    assert_eq!(capped[1].id, "r2");
}

#[quench_test::test]
fn descendant_project_ids_includes_the_root_and_everything_nested_under_it() {
    let projects = vec![
        project("root", None),
        project("mid", Some("root")),
        project("leaf", Some("mid")),
        project("cousin", None),
    ];
    let ids = descendant_project_ids("root", &projects);
    assert!(ids.contains("root") && ids.contains("mid") && ids.contains("leaf"));
    assert!(!ids.contains("cousin"));
}

#[quench_test::test]
fn repo_ids_under_only_keeps_repos_whose_project_is_in_scope() {
    let mut project_ids = HashSet::new();
    project_ids.insert("in-scope".to_string());
    let repos = vec![repo("in", "in-scope"), repo("out", "out-of-scope")];
    assert_eq!(repo_ids_under(&project_ids, &repos), vec!["in".to_string()]);
}
