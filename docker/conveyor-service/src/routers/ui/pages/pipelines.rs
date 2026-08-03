//! The full pipeline history: every run, not just the front page's capped
//! handful - paged, and optionally scoped to one project's branch of the tree
//! the same way `/projects/{id}` scopes the front page itself.

use super::shared;
use crate::config::ConveyorConfig;
use crate::domain::{Project, Repo, Run};
use crate::routers::ui::common::{
    UiPageKind, is_ui_authenticated, render_page, ui_login_redirect, ui_path,
};
use crate::scheduler::{projects, queue, repos};
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use quench_web::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct RunsListQuery {
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    project: Option<String>,
}

#[get("/runs")]
pub(super) async fn runs_list_page(
    request: HttpRequest,
    query: web::Query<RunsListQuery>,
    config: web::Data<JwtConfig>,
    conveyor_config: web::Data<ConveyorConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    if !is_ui_authenticated(&request, &config).await {
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
        HttpResponse::Ok(),
        content().class("home-content").child(page_body(
            &runs,
            &repositories,
            &all_projects,
            scope.as_ref(),
            page,
            page_count(total, page_size),
        )),
        UiPageKind::Home,
    )
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

/// How many pages `page_size` rows at a time makes of `total` rows - at least
/// one, even when `total` is zero, so an empty history still has a "page 1 of
/// 1" to land on rather than a division with nothing on either side of it.
fn page_count(total: i64, page_size: i64) -> u32 {
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

fn pager(scope: Option<&Project>, page: u32, total_pages: u32) -> Element {
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

/// A link to `target`, when there is a page to go to - otherwise the same
/// label, disabled, so the control does not shift position between "has more"
/// and "does not".
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn project(id: &str) -> Project {
        Project {
            id: id.to_string(),
            name: id.to_string(),
            parent_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn an_empty_history_is_still_page_one_of_one() {
        assert_eq!(page_count(0, 25), 1);
    }

    #[test]
    fn a_partial_last_page_still_counts_as_a_whole_page() {
        assert_eq!(page_count(26, 25), 2);
        assert_eq!(page_count(25, 25), 1);
        assert_eq!(page_count(50, 25), 2);
    }

    #[test]
    fn the_first_page_has_no_previous_link() {
        let html = pager(None, 1, 3).render();
        assert!(html.contains("pager-link-disabled"));
        assert!(!html.contains("page=0"));
    }

    #[test]
    fn the_last_page_has_no_next_link() {
        let html = pager(None, 3, 3).render();
        let next_disabled = html.rfind("pager-link-disabled").expect("a disabled link");
        // Only the trailing (next) control should be disabled on the last page.
        assert!(html[..next_disabled].contains("page=2"));
    }

    #[test]
    fn a_middle_page_links_both_ways() {
        let html = pager(None, 2, 3).render();
        assert!(html.contains("page=1"));
        assert!(html.contains("page=3"));
        assert!(!html.contains("pager-link-disabled"));
    }

    #[test]
    fn a_scoped_pager_carries_the_project_along() {
        let scope = project("lorehaven");
        let html = pager(Some(&scope), 1, 2).render();
        assert!(html.contains("project=lorehaven"));
        assert!(html.contains("page=2"));
    }
}
