//! The full pipeline history, paged - optionally scoped the same way `/projects/{id}` scopes the front page.

use super::shared;
use crate::config::ConveyorConfig;
use crate::domain::{Project, Repo, Run};
use crate::routers::ui::common::{PageAuth, render_page, ui_login_redirect, ui_path};
use crate::scheduler::{projects, queue, repos};
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Query, Response, get, http::StatusCode};
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct RunsListQuery {
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    project: Option<String>,
}

#[get("/ui/runs")]
pub(super) async fn runs_list_page(
    PageAuth(authenticated): PageAuth,
    Query(query): Query<RunsListQuery>,
    Inject(conveyor_config): Inject<ConveyorConfig>,
    Inject(db): Inject<Db>,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }

    let all_projects = projects::list_all(&db).await.unwrap_or_default();

    let scope = match &query.project {
        Some(id) => match all_projects.iter().find(|project| &project.id == id) {
            Some(project) => Some(project.clone()),
            None => return not_found(),
        },
        None => None,
    };

    let repositories = repos::list(&db).await.unwrap_or_default();
    let repo_scope = scope.as_ref().map(|project| {
        let descendants = shared::descendant_project_ids(&project.id, &all_projects);
        shared::repo_ids_under(&descendants, &repositories)
    });

    let page_size = i64::try_from(conveyor_config.runs_page_size).unwrap_or(25);
    let page = query.page.unwrap_or(1).max(1);
    let offset = i64::from(page - 1) * page_size;

    let total = queue::count_runs(&db, repo_scope.as_deref())
        .await
        .unwrap_or(0);
    let runs = queue::list_runs_page(&db, repo_scope.as_deref(), page_size, offset)
        .await
        .unwrap_or_default();

    render_page(
        StatusCode::OK,
        content().class("home-content").child(page_body(
            &runs,
            &repositories,
            &all_projects,
            scope.as_ref(),
            page,
            page_count(total, page_size),
        )),
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

/// At least 1, even for `total == 0`, so an empty history still has a "page 1 of 1".
pub fn page_count(total: i64, page_size: i64) -> u32 {
    if total <= 0 {
        return 1;
    }
    let page_size = page_size.max(1);
    let pages = (total + page_size - 1) / page_size;
    u32::try_from(pages).unwrap_or(u32::MAX).max(1)
}

fn page_body(
    runs: &[Run],
    repositories: &[Repo],
    all_projects: &[Project],
    scope: Option<&Project>,
    page: u32,
    total_pages: u32,
) -> Element {
    div()
        .class("home-container")
        .child(header(all_projects, scope))
        .child(
            div()
                .class("panel")
                .child(
                    div()
                        .class("panel-title")
                        .attr("data-i18n", "ui_pipelines_title"),
                )
                .child(shared::runs_table(runs, repositories))
                .child(pager(scope, page, total_pages)),
        )
}

fn header(all_projects: &[Project], scope: Option<&Project>) -> Element {
    let title = match scope {
        Some(project) => shared::breadcrumb(all_projects, project),
        None => h3().attr("data-i18n", "ui_pipelines_title"),
    };

    div().class("home-header").child(title).child(
        p().class("home-subtitle")
            .attr("data-i18n", "ui_pipelines_subtitle"),
    )
}

pub fn pager(scope: Option<&Project>, page: u32, total_pages: u32) -> Element {
    div()
        .class("pager")
        .child(pager_link(
            scope,
            (page > 1).then(|| page - 1),
            "ui_pager_prev",
        ))
        .child(
            span()
                .class("pager-status")
                .attr("data-i18n", "ui_pager_page")
                .attr(
                    "data-i18n-args",
                    format!("{{\"page\":\"{page}\",\"total\":\"{total_pages}\"}}"),
                ),
        )
        .child(pager_link(
            scope,
            (page < total_pages).then_some(page + 1),
            "ui_pager_next",
        ))
}

/// Disabled with the same label when there's no page to go to, so nothing shifts position.
fn pager_link(scope: Option<&Project>, target: Option<u32>, label_key: &str) -> Element {
    match target {
        Some(page) => a()
            .class("pager-link")
            .attr("href", page_href(scope, page))
            .attr("data-i18n", label_key),
        None => span()
            .class("pager-link pager-link-disabled")
            .attr("data-i18n", label_key),
    }
}

fn page_href(scope: Option<&Project>, page: u32) -> String {
    match scope {
        Some(project) => ui_path(&format!("/runs?project={}&page={page}", project.id)),
        None => ui_path(&format!("/runs?page={page}")),
    }
}

pub(super) fn register_routes() {
    let _ = runs_list_page as fn(_, _, _, _) -> _;
}
