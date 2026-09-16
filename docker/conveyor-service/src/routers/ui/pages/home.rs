//! Conveyor's front page: what has run lately, and what is registered. Also,
//! scoped, `/projects/{id}`'s own branch - same layout, rooted differently.

use super::shared;
use crate::config::ConveyorConfig;
use crate::domain::{Project, Repo, Run};
use crate::routers::ui::common::{PageAuth, PageGate, render_page, ui_login_redirect, ui_path};
use crate::scan::{CheckKind, CheckResult, ScanSummary};
use crate::scheduler::{projects, queue, repos};
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode, post};
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use serde::Deserialize;
use std::collections::HashMap;

/// Pulled before the per-repo cap ([`ConveyorConfig::home_max_runs_per_repo`])
/// applies - generous enough a quiet repo isn't crowded off by a noisy one.
const FETCH: i64 = 200;

/// Slower than the run page's and never stops - a dashboard with no resting state.
const POLL_INTERVAL: &str = "every 5s";

/// Unscoped (front page) or scoped to one project's branch.
#[derive(Deserialize)]
pub(super) struct ScopeQuery {
    #[serde(default)]
    project: Option<String>,
}

#[get("/ui/home")]
pub(super) async fn home(
    auth: PageAuth,
    Inject(conveyor_config): Inject<ConveyorConfig>,
    Inject(db): Inject<Db>,
) -> Response {
    render(auth, conveyor_config, db, None).await
}

#[get("/ui/home/")]
pub(super) async fn home_slash(
    auth: PageAuth,
    Inject(conveyor_config): Inject<ConveyorConfig>,
    Inject(db): Inject<Db>,
) -> Response {
    render(auth, conveyor_config, db, None).await
}

/// Same layout, rooted at one node - see `project_node` for what links here.
#[get("/ui/projects/{id}")]
pub(super) async fn project_page(
    auth: PageAuth,
    Path(id): Path<String>,
    Inject(conveyor_config): Inject<ConveyorConfig>,
    Inject(db): Inject<Db>,
) -> Response {
    render(auth, conveyor_config, db, Some(id)).await
}

async fn render(
    PageAuth(authenticated): PageAuth,
    conveyor_config: std::sync::Arc<ConveyorConfig>,
    db: std::sync::Arc<Db>,
    scope_id: Option<String>,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }

    let all_projects = projects::list_all(&db).await.unwrap_or_default();

    let scope = match &scope_id {
        Some(id) => match all_projects.iter().find(|project| &project.id == id) {
            Some(project) => Some(project.clone()),
            None => return not_found(),
        },
        None => None,
    };

    let repositories = repos::list(&db).await.unwrap_or_default();
    let scans = shared::scan_summaries(&db, &repositories).await;

    let repo_scope = scope.as_ref().map(|project| {
        let descendants = shared::descendant_project_ids(&project.id, &all_projects);
        shared::repo_ids_under(&descendants, &repositories)
    });

    let runs = queue::list_runs_page(&db, repo_scope.as_deref(), FETCH, 0)
        .await
        .unwrap_or_default();
    let runs = shared::cap_per_repo(
        &runs,
        conveyor_config.home_recent_runs,
        conveyor_config.home_max_runs_per_repo,
    );

    render_page(
        StatusCode::OK,
        content().class("home-content").child(page(
            &runs,
            &repositories,
            &all_projects,
            &scans,
            scope.as_ref(),
        )),
    )
}

/// Rendered by the page and fragment alike, so the swap can't drift - the
/// tree used to live here too, but polling it would collapse open nodes.
fn runs_section(runs: &[Run], repositories: &[Repo], scope: Option<&str>) -> Element {
    div()
        .attr("id", "home-state")
        .attr("hx-get", state_href(scope))
        .attr("hx-trigger", POLL_INTERVAL)
        .attr("hx-swap", "outerHTML")
        .child(runs_panel(runs, repositories, scope))
}

fn state_href(scope: Option<&str>) -> String {
    match scope {
        Some(project) => ui_path(&format!("/home/state?project={project}")),
        None => ui_path("/home/state"),
    }
}

fn view_all_href(scope: Option<&str>) -> String {
    match scope {
        Some(project) => ui_path(&format!("/runs?project={project}")),
        None => ui_path("/runs"),
    }
}

/// `Some(&[])` for a project that no longer exists, not a fallback to unscoped.
async fn resolve_scope(db: &Db, project: Option<&str>) -> Option<Vec<String>> {
    let project_id = project?;
    let all_projects = projects::list_all(db).await.unwrap_or_default();
    if !all_projects.iter().any(|p| p.id == project_id) {
        return Some(Vec::new());
    }
    let repositories = repos::list(db).await.unwrap_or_default();
    let descendants = shared::descendant_project_ids(project_id, &all_projects);
    Some(shared::repo_ids_under(&descendants, &repositories))
}

/// The polled half of the front page - and of any project's own branch of it.
#[get("/ui/home/state")]
pub(super) async fn home_state(
    gate: PageGate,
    Query(query): Query<ScopeQuery>,
    Inject(conveyor_config): Inject<ConveyorConfig>,
    Inject(db): Inject<Db>,
) -> Response {
    // The fragment-aware form - polled every 5s, likeliest to meet an expired session.
    if let Err(response) = gate.or_redirect() {
        return response;
    }

    let repositories = repos::list(&db).await.unwrap_or_default();
    let repo_scope = resolve_scope(&db, query.project.as_deref()).await;
    let runs = queue::list_runs_page(&db, repo_scope.as_deref(), FETCH, 0)
        .await
        .unwrap_or_default();
    let runs = shared::cap_per_repo(
        &runs,
        conveyor_config.home_recent_runs,
        conveyor_config.home_max_runs_per_repo,
    );

    Response::html(
        StatusCode::OK,
        runs_section(&runs, &repositories, query.project.as_deref()).render(),
    )
}

/// Hands back the same fragment `/home/state` polls, so the button's own
/// request settles the table immediately rather than waiting for the next poll.
#[post("/ui/home/repos/{repo_id}/run")]
pub(super) async fn run_now(
    gate: PageGate,
    Path(repo_id): Path<String>,
    Query(query): Query<ScopeQuery>,
    Inject(conveyor_config): Inject<ConveyorConfig>,
    Inject(db): Inject<Db>,
) -> Response {
    if let Err(response) = gate.or_redirect() {
        return response;
    }

    if let Err(error) = crate::routers::api::runs::trigger_manual(&db, &repo_id, None, None).await {
        tracing::warn!("manual run for repo {repo_id} could not start: {error}");
    }

    let repositories = repos::list(&db).await.unwrap_or_default();
    let repo_scope = resolve_scope(&db, query.project.as_deref()).await;
    let runs = queue::list_runs_page(&db, repo_scope.as_deref(), FETCH, 0)
        .await
        .unwrap_or_default();
    let runs = shared::cap_per_repo(
        &runs,
        conveyor_config.home_recent_runs,
        conveyor_config.home_max_runs_per_repo,
    );

    Response::html(
        StatusCode::OK,
        runs_section(&runs, &repositories, query.project.as_deref()).render(),
    )
}

fn not_found() -> Response {
    render_page(
        StatusCode::NOT_FOUND,
        content().class("home-content").child(
            div()
                .class("home-container")
                .child(empty_state("ui_project_not_found")),
        ),
    )
}

fn page(
    runs: &[Run],
    repositories: &[Repo],
    all_projects: &[Project],
    scans: &HashMap<String, ScanSummary>,
    scope: Option<&Project>,
) -> Element {
    let scope_id = scope.map(|project| project.id.as_str());

    div()
        .class("home-container")
        .child(header_row(all_projects, scope))
        .child(runs_section(runs, repositories, scope_id))
        .child(project_tree_panel_scoped(
            all_projects,
            repositories,
            scans,
            scope_id,
        ))
}

fn header_row(all_projects: &[Project], scope: Option<&Project>) -> Element {
    let Some(project) = scope else {
        return div()
            .class("home-header")
            .child(h3().attr("data-i18n", "ui_home_title"))
            .child(
                p().class("home-subtitle")
                    .attr("data-i18n", "ui_home_subtitle"),
            );
    };

    div()
        .class("home-header")
        .child(shared::breadcrumb(all_projects, project))
        .child(
            p().class("home-subtitle")
                .attr("data-i18n", "ui_project_subtitle"),
        )
}

fn runs_panel(runs: &[Run], repositories: &[Repo], scope: Option<&str>) -> Element {
    div()
        .class("panel")
        .child(
            div()
                .class("panel-title panel-title-row")
                .child(span().attr("data-i18n", "ui_runs_title"))
                .child(
                    a().class("panel-title-link")
                        .attr("href", view_all_href(scope))
                        .attr("data-i18n", "ui_runs_view_all"),
                ),
        )
        .child(shared::runs_table(runs, repositories))
}

/// The registered projects as the tree they form - rendered once per page
/// load, not polled; see `runs_section` for why that split exists.
pub fn project_tree_panel(
    all_projects: &[Project],
    repositories: &[Repo],
    scans: &HashMap<String, ScanSummary>,
) -> Element {
    project_tree_panel_scoped(all_projects, repositories, scans, None)
}

/// `project_tree_panel`, rooted at `root_id` instead of the top; `root_id`
/// itself isn't shown, since `breadcrumb` already says where it is.
pub fn project_tree_panel_scoped(
    all_projects: &[Project],
    repositories: &[Repo],
    scans: &HashMap<String, ScanSummary>,
    root_id: Option<&str>,
) -> Element {
    let panel = div().class("panel").child(
        div()
            .class("panel-title panel-title-row")
            .child(span().attr("data-i18n", "ui_repos_title"))
            .child(
                a().class("panel-title-link")
                    .attr("href", ui_path("/repos"))
                    .attr("data-i18n", "ui_repos_manage"),
            ),
    );

    let mut children_of: HashMap<Option<&str>, Vec<&Project>> = HashMap::new();
    for project in all_projects {
        children_of
            .entry(project.parent_id.as_deref())
            .or_default()
            .push(project);
    }

    let mut repos_of: HashMap<&str, Vec<&Repo>> = HashMap::new();
    for repo in repositories {
        repos_of
            .entry(repo.project_id.as_str())
            .or_default()
            .push(repo);
    }

    let mut elements = Vec::new();
    if let Some(id) = root_id
        && let Some(repos) = repos_of.get(id)
    {
        elements.push(repo_table(repos, scans, root_id));
    }

    let roots = children_of.get(&root_id).cloned().unwrap_or_default();
    elements.extend(render_level(
        &roots,
        &children_of,
        &repos_of,
        scans,
        root_id,
    ));

    if elements.is_empty() {
        return panel.child(empty_state("ui_repos_empty"));
    }

    let mut tree = div().class("project-tree");
    for element in elements {
        tree = tree.child(element);
    }

    panel.child(tree)
}

/// One level of the tree, sorted into what needs a disclosure and what
/// doesn't - a leaf's repo (if any) folds into one shared table instead.
fn render_level(
    nodes: &[&Project],
    children_of: &HashMap<Option<&str>, Vec<&Project>>,
    repos_of: &HashMap<&str, Vec<&Repo>>,
    scans: &HashMap<String, ScanSummary>,
    scope: Option<&str>,
) -> Vec<Element> {
    let mut leaf_repos = Vec::new();
    let mut empty_leaves = Vec::new();
    let mut containers = Vec::new();

    for &node in nodes {
        let is_container = children_of
            .get(&Some(node.id.as_str()))
            .is_some_and(|children| !children.is_empty());

        if is_container {
            containers.push(project_node(node, children_of, repos_of, scans, scope));
        } else if let Some(repos) = repos_of.get(node.id.as_str()) {
            leaf_repos.extend(repos.iter().copied());
        } else {
            // Registered but empty - still worth showing, just as its own link.
            empty_leaves.push(node);
        }
    }

    let mut elements = Vec::new();
    if !leaf_repos.is_empty() {
        elements.push(repo_table(&leaf_repos, scans, scope));
    }
    for empty in empty_leaves {
        // Muted, not bold - still a plain leaf, not a promotion to a container.
        elements.push(
            a().class("muted project-leaf-link")
                .attr("href", ui_path(&format!("/projects/{}", empty.id)))
                .text(&empty.name),
        );
    }
    elements.extend(containers);
    elements
}

/// A link to a project's own branch of this page - the tree's whole reason
/// for making container nodes clickable in the first place.
fn project_link(project: &Project) -> Element {
    a().class("project-name")
        .attr("href", ui_path(&format!("/projects/{}", project.id)))
        .text(&project.name)
}

/// A native `<details>` disclosure - only called when `render_level` finds children.
fn project_node(
    project: &Project,
    children_of: &HashMap<Option<&str>, Vec<&Project>>,
    repos_of: &HashMap<&str, Vec<&Repo>>,
    scans: &HashMap<String, ScanSummary>,
    scope: Option<&str>,
) -> Element {
    let mut node = element("details")
        .class("project-node")
        .attr("open", "open")
        .child(
            element("summary")
                .class("project-head")
                .child(project_link(project)),
        );

    // Rare: a container holding its own repository directly, not through a child.
    if let Some(repos) = repos_of.get(project.id.as_str()) {
        node = node.child(repo_table(repos, scans, scope));
    }

    let children = children_of
        .get(&Some(project.id.as_str()))
        .cloned()
        .unwrap_or_default();
    let mut nested = div().class("project-children");
    for element in render_level(&children, children_of, repos_of, scans, scope) {
        nested = nested.child(element);
    }

    node.child(nested)
}

fn repo_table(
    repos: &[&Repo],
    scans: &HashMap<String, ScanSummary>,
    scope: Option<&str>,
) -> Element {
    let mut table = element("table").class("run-table").child(
        element("tr")
            .child(element("th").attr("data-i18n", "ui_col_repository"))
            .child(element("th").attr("data-i18n", "ui_col_provider"))
            .child(element("th").attr("data-i18n", "ui_col_branch"))
            .child(element("th").attr("data-i18n", "ui_col_state"))
            .child(element("th").attr("data-i18n", "ui_col_checks"))
            .child(element("th").attr("data-i18n", "ui_col_actions")),
    );

    for repo in repos {
        table = table.child(repo_row(repo, scans.get(&repo.id), scope));
    }

    table
}

/// Just `repo.name` - tree nesting already shows where it sits; the `href` still needs the full identity.
fn repo_row(repo: &Repo, scan: Option<&ScanSummary>, scope: Option<&str>) -> Element {
    element("tr")
        .child(
            element("td").child(
                a().attr(
                    "href",
                    ui_path(&format!("/repos/{}/{}/scan", repo.owner, repo.name)),
                )
                .text(&repo.name),
            ),
        )
        .child(element("td").class("muted").text(repo.provider.to_string()))
        .child(element("td").class("mono").text(&repo.default_branch))
        .child(element("td").child(if repo.enabled {
            span()
                .class("status status-success")
                .attr("data-i18n", "ui_repo_enabled")
        } else {
            span()
                .class("status status-skipped")
                .attr("data-i18n", "ui_repo_disabled")
        }))
        .child(element("td").child(check_chips(scan)))
        .child(element("td").child(run_button(repo, scope)))
}

/// One chip per check, colored by what the most recent run found - the counts mirror the scan page's cards.
pub fn check_chips(summary: Option<&ScanSummary>) -> Element {
    div()
        .class("chip-row")
        .child(chip(CheckKind::Lint, summary.and_then(|s| s.lint.as_ref())))
        .child(chip(
            CheckKind::Machete,
            summary.and_then(|s| s.machete.as_ref()),
        ))
        .child(chip(
            CheckKind::Audit,
            summary.and_then(|s| s.audit.as_ref()),
        ))
        .child(chip(
            CheckKind::Coverage,
            summary.and_then(|s| s.coverage.as_ref()),
        ))
}

pub fn chip(kind: CheckKind, check: Option<&CheckResult>) -> Element {
    let letter = match kind {
        CheckKind::Lint => "L",
        CheckKind::Machete => "M",
        CheckKind::Audit => "A",
        CheckKind::Coverage => "C",
    };

    let count = |check: &CheckResult| {
        check
            .metric
            .clone()
            .unwrap_or_else(|| check.findings.len().to_string())
    };

    let (severity_class, label, title_key) = match check {
        None => ("chip-none", "-".to_string(), "ui_chip_not_run"),
        Some(check) if check.findings.is_empty() => ("chip-clean", count(check), kind.label()),
        Some(check) if check.passed => ("chip-warning", count(check), kind.label()),
        Some(check) => ("chip-danger", count(check), kind.label()),
    };

    span()
        .class(format!("chip {severity_class}"))
        .attr("data-i18n-title", title_key)
        .text(format!("{letter} {label}"))
}

/// No button at all when disabled - it would just 409, and that's worse than nothing.
pub fn run_button(repo: &Repo, scope: Option<&str>) -> Element {
    if !repo.enabled {
        return span();
    }

    let href = match scope {
        Some(project) => format!("/home/repos/{}/run?project={project}", repo.id),
        None => format!("/home/repos/{}/run", repo.id),
    };

    button()
        .attr("type", "button")
        .class("run-button")
        .attr("data-i18n", "ui_repo_run_now")
        .attr("hx-post", ui_path(&href))
        .attr("hx-target", "#home-state")
        .attr("hx-swap", "outerHTML")
}

pub(super) fn register_routes() {
    let _ = home as fn(_, _, _) -> _;
    let _ = home_slash as fn(_, _, _) -> _;
    let _ = project_page as fn(_, _, _, _) -> _;
    let _ = home_state as fn(_, _, _, _) -> _;
    let _ = run_now as fn(_, _, _, _, _) -> _;
}
