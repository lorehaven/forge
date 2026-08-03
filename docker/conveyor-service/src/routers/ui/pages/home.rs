//! Conveyor's front page: what has run lately, and what is registered.
//!
//! Also, in scoped form, one project's own branch of both: `/projects/{id}`
//! renders the exact same layout with the tree rooted at that project and the
//! run list filtered to what sits under it, rather than a second page that
//! could drift from this one.

use super::shared;
use crate::config::ConveyorConfig;
use crate::domain::{Project, Repo, Run};
use crate::routers::ui::common::{
    UiPageKind, is_ui_authenticated, render_page, ui_login_redirect_for, ui_path,
};
use crate::scan::{CheckKind, CheckResult, ScanSummary};
use crate::scheduler::{projects, queue, repos};
use actix_web::{HttpResponse, Responder, get, http::header::ContentType, post, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use quench_starter::actix::routers::ui::pages::home::handle_home;
use quench_web::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

/// How many of the newest runs are pulled from the database before the
/// per-repository cap ([`ConveyorConfig::home_max_runs_per_repo`]) is applied.
/// Generous enough that a handful of quiet repositories are not crowded off
/// the front page by one noisy one, without pulling the run table's whole
/// history for a page that only ever shows a few rows of it.
const FETCH: i64 = 200;

/// Slower than the run page's, and it never stops: this is a dashboard with no
/// resting state to reach, where a run appearing a few seconds late costs
/// nothing. Both panels are whole-state replacements with no stream inside
/// them, so the swap can take the lot.
const POLL_INTERVAL: &str = "every 5s";

/// What a fragment or action route needs to know to answer for the right
/// page: unscoped (the front page) or scoped to one project's branch.
#[derive(Deserialize)]
pub(super) struct ScopeQuery {
    #[serde(default)]
    project: Option<String>,
}

#[get("/home")]
pub(super) async fn home(
    request: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    conveyor_config: web::Data<ConveyorConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    render(request, config, conveyor_config, db, None).await
}

#[get("/home/")]
pub(super) async fn home_slash(
    request: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    conveyor_config: web::Data<ConveyorConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    render(request, config, conveyor_config, db, None).await
}

/// A project's own branch of the front page: the same layout, rooted at one
/// node instead of the whole tree. What node is clicked in the tree - see
/// `project_node` - is what lands here.
#[get("/projects/{id}")]
pub(super) async fn project_page(
    request: actix_web::HttpRequest,
    path: web::Path<String>,
    config: web::Data<JwtConfig>,
    conveyor_config: web::Data<ConveyorConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    render(
        request,
        config,
        conveyor_config,
        db,
        Some(path.into_inner()),
    )
    .await
}

async fn render(
    request: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    conveyor_config: web::Data<ConveyorConfig>,
    db: web::Data<Db>,
    scope_id: Option<String>,
) -> HttpResponse {
    // Read before the auth check so the closure `handle_home` takes has
    // everything it needs; an unauthenticated caller is redirected either way
    // and the reads are cheap.
    let all_projects = projects::list_all(&db).await.unwrap_or_default();

    let scope = match &scope_id {
        Some(id) => match all_projects.iter().find(|project| &project.id == id) {
            Some(project) => Some(project.clone()),
            // Same treatment as an unauthenticated request: the closure
            // decides what to render, `handle_home` only gates whether it
            // runs at all - a caller with no session learns nothing about
            // whether the id exists either way.
            None => return handle_home(request, config, not_found).await,
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

    handle_home(request, config, || {
        render_page(
            HttpResponse::Ok(),
            content().class("home-content").child(page(
                &runs,
                &repositories,
                &all_projects,
                &scans,
                scope.as_ref(),
            )),
            UiPageKind::Home,
        )
    })
    .await
}

/// The runs panel, and the element that asks for it again. Rendered by the
/// page and by the fragment alike, so the swap cannot drift from the first
/// paint.
///
/// The project tree used to live in here too. It does not any more: nothing
/// about a project's shape or a repository's scan results changes on the
/// timescale a run does, and re-rendering the tree every few seconds would
/// only throw away whatever a visitor had expanded - `<details>` gives that
/// for free for as long as nothing keeps replacing it out from under itself.
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

/// Resolves the scoped repository set a fragment or action route was asked
/// to answer for. `Some(&[])` when the query names a project that no longer
/// exists - an empty, clearly-scoped result rather than silently falling back
/// to the unscoped view of everything.
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
#[get("/home/state")]
pub(super) async fn home_state(
    request: actix_web::HttpRequest,
    query: web::Query<ScopeQuery>,
    config: web::Data<JwtConfig>,
    conveyor_config: web::Data<ConveyorConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    if !is_ui_authenticated(&request, &config).await {
        return ui_login_redirect_for(&request);
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

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(runs_section(&runs, &repositories, query.project.as_deref()).render())
}

/// Starts a run for one repository by hand, then hands back the same fragment
/// `/home/state` polls - the button's own request settles the table it sits
/// in, rather than waiting for the next poll to notice.
#[post("/home/repos/{repo_id}/run")]
pub(super) async fn run_now(
    request: actix_web::HttpRequest,
    path: web::Path<String>,
    query: web::Query<ScopeQuery>,
    config: web::Data<JwtConfig>,
    conveyor_config: web::Data<ConveyorConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    if !is_ui_authenticated(&request, &config).await {
        return ui_login_redirect_for(&request);
    }

    let repo_id = path.into_inner();
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

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(runs_section(&runs, &repositories, query.project.as_deref()).render())
}

fn not_found() -> HttpResponse {
    render_page(
        HttpResponse::NotFound(),
        content().class("home-content").child(
            div().class("home-container").child(
                div()
                    .class("empty")
                    .attr("data-i18n", "ui_project_not_found"),
            ),
        ),
        UiPageKind::Home,
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

/// The registered projects, as the tree they actually form. A project may
/// contain other projects (nesting is unbounded), a repository (zero or one),
/// both, or neither - a leaf and a container are the same element, they just
/// end up with different children.
///
/// Rendered once per page load rather than polled - see `runs_section` for
/// why that split exists.
pub fn project_tree_panel(
    all_projects: &[Project],
    repositories: &[Repo],
    scans: &HashMap<String, ScanSummary>,
) -> Element {
    project_tree_panel_scoped(all_projects, repositories, scans, None)
}

/// `project_tree_panel`, rooted somewhere other than the top of the tree.
/// `root_id`'s own directly-attached repository (if it has one) and its
/// children are shown; `root_id` itself is not, since a scoped page's header
/// already says where it is - see `breadcrumb`.
pub fn project_tree_panel_scoped(
    all_projects: &[Project],
    repositories: &[Repo],
    scans: &HashMap<String, ScanSummary>,
    root_id: Option<&str>,
) -> Element {
    let panel = div().class("panel").child(
        div()
            .class("panel-title")
            .attr("data-i18n", "ui_repos_title"),
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
        return panel.child(div().class("empty").attr("data-i18n", "ui_repos_empty"));
    }

    let mut tree = div().class("project-tree");
    for element in elements {
        tree = tree.child(element);
    }

    panel.child(tree)
}

/// One level of the tree: every child of some node (or the roots), sorted
/// into what actually needs a disclosure and what does not.
///
/// A project with no projects of its own nested under it is a leaf, whatever
/// it holds - and a leaf gets no `<details>` of its own. Expanding it would
/// only ever reveal the one table row it already is; that is not a level to
/// step into, it is the repository itself. So every leaf's repo (if it has
/// one) folds straight into one shared table at this level, the way flat rows
/// already worked before there was a tree at all - only a node with its own
/// children still needs the fold-out treatment, because that is the one case
/// where there is somewhere further to go.
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
            // A registered project with nothing in it yet - still worth
            // showing, since it exists, but with no repo to tabulate and
            // nothing nested to disclose, it is just its own link.
            empty_leaves.push(node);
        }
    }

    let mut elements = Vec::new();
    if !leaf_repos.is_empty() {
        elements.push(repo_table(&leaf_repos, scans, scope));
    }
    for empty in empty_leaves {
        // Muted rather than bold: this is still a plain leaf, just one with
        // nothing to tabulate - the link is what makes it worth having a row
        // for at all now, not a promotion to looking like a container.
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

/// A container node: a native disclosure holding whatever is nested under it.
/// Only called for a project that actually has children - `render_level` is
/// what decides that, so by the time this runs there is always something to
/// fold out. `<details>` gives collapse and expand with no script at all,
/// same as the run page's job list.
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

    // Rare: a container holding a repository of its own, directly, rather
    // than through a child - the schema allows it even though nothing in this
    // tree's example data does.
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

/// Just `repo.name` - the tree nesting is what shows where it sits (down to
/// its owner, if that owner is itself part of the tree), so repeating the
/// full `owner/name` slug in every row would only say the same thing twice.
/// The link's `href` still needs the full identity, since that is what
/// resolves a specific repository regardless of where in the tree it is.
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

/// One small chip per check this page knows about, coloured by what the most
/// recent run found - not run at all, clean, still passing with findings, or
/// failed outright. The counts mirror the scan page's cards; the color is
/// what a chip adds that a bare number does not.
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
}

pub fn chip(kind: CheckKind, check: Option<&CheckResult>) -> Element {
    let letter = match kind {
        CheckKind::Lint => "L",
        CheckKind::Machete => "M",
        CheckKind::Audit => "A",
    };

    let (severity_class, label, title_key) = match check {
        None => ("chip-none", "-".to_string(), "ui_chip_not_run"),
        Some(check) if check.findings.is_empty() => ("chip-clean", "0".to_string(), kind.label()),
        Some(check) if check.passed => (
            "chip-warning",
            check.findings.len().to_string(),
            kind.label(),
        ),
        Some(check) => (
            "chip-danger",
            check.findings.len().to_string(),
            kind.label(),
        ),
    };

    span()
        .class(format!("chip {severity_class}"))
        .attr("data-i18n-title", title_key)
        .text(format!("{letter} {label}"))
}

/// Queues a run for the repository's default branch. Disabled repositories
/// get no button at all - the same request would just come back with a 409
/// from the API this calls, and a button that always fails is worse than no
/// button.
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
