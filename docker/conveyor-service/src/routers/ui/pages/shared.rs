//! Bits the home page and the pages that scope or paginate it share.
//!
//! `home.rs` renders the whole estate, and - at `/projects/{id}` - one branch
//! of it; `pipelines.rs` renders one long, paged table of either. All three
//! read the same rows and draw the same table, so the reading and the drawing
//! live here once rather than twice or three times over.

use crate::domain::{Project, Repo, Run};
use crate::routers::ui::common::{format, status_pill, ui_path};
use crate::scan::ScanSummary;
use futures_util::future::join_all;
use quench_db::prelude::Db;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use std::collections::{HashMap, HashSet};

/// One lookup per repository, run concurrently - `scan::latest` is several
/// sequential DB round trips on its own, and no page here has any other use
/// for waiting on them one repository at a time.
pub async fn scan_summaries(db: &Db, repositories: &[Repo]) -> HashMap<String, ScanSummary> {
    let fetches = repositories.iter().map(|repo| async move {
        let summary = crate::scan::latest(db, &repo.id).await.unwrap_or_default();
        (repo.id.clone(), summary)
    });
    join_all(fetches).await.into_iter().collect()
}

/// Keeps the newest run per repository - at most `max_per_repo` of them - and
/// caps the whole selection at `max_total`. `runs` must already be sorted
/// newest first; that is the order a repository's "newest" and the list's
/// "first" both mean.
///
/// Cloning rather than borrowing: every caller wants an owned `Vec<Run>` it
/// can render immediately, and a front page's run count is small enough that
/// the copies cost nothing worth avoiding the borrow-checker fight for.
pub fn cap_per_repo(runs: &[Run], max_total: usize, max_per_repo: usize) -> Vec<Run> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut selected = Vec::new();

    for run in runs {
        if selected.len() >= max_total {
            break;
        }
        let count = seen.entry(run.repo_id.as_str()).or_insert(0);
        if *count >= max_per_repo {
            continue;
        }
        *count += 1;
        selected.push(run.clone());
    }

    selected
}

/// `root_id` and every project nested under it, however deep - the set a
/// scoped page's tree and run list both filter down to.
pub fn descendant_project_ids(root_id: &str, all_projects: &[Project]) -> HashSet<String> {
    let mut children_of: HashMap<&str, Vec<&Project>> = HashMap::new();
    for project in all_projects {
        if let Some(parent_id) = &project.parent_id {
            children_of
                .entry(parent_id.as_str())
                .or_default()
                .push(project);
        }
    }

    let mut ids = HashSet::new();
    let mut stack = vec![root_id];
    while let Some(id) = stack.pop() {
        if !ids.insert(id.to_string()) {
            continue;
        }
        if let Some(children) = children_of.get(id) {
            stack.extend(children.iter().map(|child| child.id.as_str()));
        }
    }

    ids
}

/// The repositories attached anywhere under `project_ids`, as the id list
/// `queue::list_runs_page` and `queue::count_runs` scope by.
pub fn repo_ids_under(project_ids: &HashSet<String>, repositories: &[Repo]) -> Vec<String> {
    repositories
        .iter()
        .filter(|repo| project_ids.contains(&repo.project_id))
        .map(|repo| repo.id.clone())
        .collect()
}

/// `id`'s ancestors, root first, with `id` itself last - a breadcrumb reads
/// left to right from the estate's root down to where the visitor is.
/// Empty when `id` names no project.
pub fn ancestor_chain<'a>(id: &str, all_projects: &'a [Project]) -> Vec<&'a Project> {
    let by_id: HashMap<&str, &Project> = all_projects
        .iter()
        .map(|project| (project.id.as_str(), project))
        .collect();

    let mut chain = Vec::new();
    let mut current = by_id.get(id).copied();
    while let Some(project) = current {
        chain.push(project);
        current = project
            .parent_id
            .as_deref()
            .and_then(|parent_id| by_id.get(parent_id).copied());
    }

    chain.reverse();
    chain
}

/// A table of runs, newest first as given - or the empty-state message, when
/// there are none. Shared by the front page's capped panel and the full
/// pipeline history page's paged one, so the columns cannot drift apart.
pub fn runs_table(runs: &[Run], repositories: &[Repo]) -> Element {
    if runs.is_empty() {
        return empty_state("ui_runs_empty");
    }

    let by_id: HashMap<&str, &Repo> = repositories
        .iter()
        .map(|repo| (repo.id.as_str(), repo))
        .collect();

    let mut table = element("table").class("run-table").child(
        element("tr")
            .child(element("th").attr("data-i18n", "ui_col_status"))
            .child(element("th").attr("data-i18n", "ui_col_repository"))
            .child(element("th").attr("data-i18n", "ui_col_ref"))
            .child(element("th").attr("data-i18n", "ui_col_commit"))
            .child(element("th").attr("data-i18n", "ui_col_trigger"))
            .child(element("th").attr("data-i18n", "ui_col_when")),
    );

    for run in runs {
        let slug = by_id
            .get(run.repo_id.as_str())
            .map_or_else(|| "-".to_string(), |repo| repo.slug());

        table = table.child(
            element("tr")
                .child(element("td").child(status_pill(run.status)))
                .child(
                    element("td").child(
                        a().attr("href", ui_path(&format!("/runs/{}", run.id)))
                            .text(slug),
                    ),
                )
                .child(element("td").class("mono").text(run.ref_name()))
                .child(element("td").class("mono muted").text(run.short_sha()))
                .child(element("td").class("muted").text(run.trigger.to_string()))
                .child(
                    element("td")
                        .class("muted")
                        .text(format::relative(run.queued_at)),
                ),
        );
    }

    table
}

/// `Home / root / ... / project` - a trail back up the tree, each segment but
/// the last a link to that ancestor's own branch of the page it heads (the
/// project page, or the pipeline list scoped to it).
pub fn breadcrumb(all_projects: &[Project], project: &Project) -> Element {
    let chain = ancestor_chain(&project.id, all_projects);

    let mut nav = h3().class("breadcrumb").child(
        a().attr("href", ui_path("/home"))
            .attr("data-i18n", "ui_home_button"),
    );

    let last = chain.len().saturating_sub(1);
    for (index, node) in chain.iter().enumerate() {
        nav = nav.child(span().class("breadcrumb-sep").text("/"));
        nav = nav.child(if index == last {
            span().class("breadcrumb-current").text(&node.name)
        } else {
            a().attr("href", ui_path(&format!("/projects/{}", node.id)))
                .text(&node.name)
        });
    }

    nav
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Provider, Status, Trigger};
    use chrono::Utc;

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
}
