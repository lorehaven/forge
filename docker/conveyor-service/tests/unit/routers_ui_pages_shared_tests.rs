use chrono::Utc;
use conveyor_service::domain::{Project, Provider, Repo, Run, Status, Trigger};
use conveyor_service::routers::ui::pages::shared::{
    ancestor_chain, breadcrumb, cap_per_repo, descendant_project_ids, repo_ids_under,
};
use std::collections::HashSet;

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
        name: id.to_string(),
        clone_url: format!("https://github.com/lorehaven/{id}.git"),
        default_branch: "main".to_string(),
        registered_by: "admin".to_string(),
        project_id: project_id.to_string(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
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
        queued_at: Utc::now(),
        started_at: None,
        finished_at: None,
        claimed_by: None,
        claimed_at: None,
        attempt: 0,
        error: None,
        resumed_from: None,
    }
}

#[test]
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

#[test]
fn cap_per_repo_stops_at_the_total_even_with_room_left_per_repo() {
    let runs = vec![run("r1", "a"), run("r2", "b"), run("r3", "c")];
    let capped = cap_per_repo(&runs, 2, 5);
    assert_eq!(capped.len(), 2);
}

#[test]
fn cap_per_repo_can_keep_more_than_one_per_repository() {
    let runs = vec![run("r3", "a"), run("r2", "a"), run("r1", "a")];
    let capped = cap_per_repo(&runs, 5, 2);
    assert_eq!(capped.len(), 2);
    assert_eq!(capped[0].id, "r3");
    assert_eq!(capped[1].id, "r2");
}

#[test]
fn descendant_project_ids_includes_the_root_and_everything_nested_under_it() {
    let projects = vec![
        project("root", "root", None),
        project("mid", "mid", Some("root")),
        project("leaf", "leaf", Some("mid")),
        project("cousin", "cousin", None),
    ];

    let ids = descendant_project_ids("root", &projects);

    assert!(ids.contains("root"));
    assert!(ids.contains("mid"));
    assert!(ids.contains("leaf"));
    assert!(!ids.contains("cousin"));
}

#[test]
fn repo_ids_under_only_keeps_repos_whose_project_is_in_scope() {
    let mut project_ids = HashSet::new();
    project_ids.insert("in-scope".to_string());
    let repos = vec![repo("in", "in-scope"), repo("out", "out-of-scope")];

    let ids = repo_ids_under(&project_ids, &repos);

    assert_eq!(ids, vec!["in".to_string()]);
}

#[test]
fn ancestor_chain_runs_root_first_down_to_the_node_itself() {
    let projects = vec![
        project("root", "root", None),
        project("mid", "mid", Some("root")),
        project("leaf", "leaf", Some("mid")),
    ];

    let chain = ancestor_chain("leaf", &projects);
    let names: Vec<&str> = chain.iter().map(|p| p.name.as_str()).collect();

    assert_eq!(names, vec!["root", "mid", "leaf"]);
}

#[test]
fn ancestor_chain_of_an_unknown_id_is_empty() {
    let projects = vec![project("root", "root", None)];
    assert!(ancestor_chain("missing", &projects).is_empty());
}

#[test]
fn breadcrumb_links_every_ancestor_but_the_last() {
    let projects = vec![
        project("root", "lorehaven", None),
        project("leaf", "forge", Some("root")),
    ];

    let html = breadcrumb(&projects, &projects[1]).render();

    assert!(html.contains("lorehaven"));
    assert!(html.contains("/ui/projects/root"));
    // The current node is text, not a link to itself.
    assert!(!html.contains("/ui/projects/leaf"));
    assert!(html.contains("forge"));
}
