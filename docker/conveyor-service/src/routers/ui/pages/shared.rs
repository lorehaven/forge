//! Bits `home.rs`, its `/projects/{id}` branch, and `pipelines.rs` all share -
//! same rows, same table, drawn once rather than three times over.

use crate::domain::{Project, Repo, Run};
use crate::routers::ui::common::{format, status_pill, ui_path};
use crate::scan::ScanSummary;
use futures_util::future::join_all;
use quench_db::prelude::Db;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use std::collections::{HashMap, HashSet};

/// Run concurrently - `scan::latest` is several sequential round trips on its own.
pub async fn scan_summaries(db: &Db, repositories: &[Repo]) -> HashMap<String, ScanSummary> {
    let fetches = repositories.iter().map(|repo| async move {
        let summary = crate::scan::latest(db, &repo.id).await.unwrap_or_default();
        (repo.id.clone(), summary)
    });
    join_all(fetches).await.into_iter().collect()
}

/// Newest `max_per_repo` runs per repo, capped at `max_total` total. `runs`
/// must already be sorted newest first. Clones - cheap at front-page scale.
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

/// `root_id` and every project nested under it, however deep.
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

/// The id list `queue::list_runs_page`/`count_runs` scope by.
pub fn repo_ids_under(project_ids: &HashSet<String>, repositories: &[Repo]) -> Vec<String> {
    repositories
        .iter()
        .filter(|repo| project_ids.contains(&repo.project_id))
        .map(|repo| repo.id.clone())
        .collect()
}

/// `id`'s ancestors, root first, `id` itself last. Empty if `id` names no project.
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

/// Shared by the front page's capped panel and the full history's paged one, so columns can't drift apart.
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

/// `Home / root / ... / project`, each segment but the last a link back up the tree.
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
