//! Conveyor's front page: what has run lately, and what is registered.

use crate::domain::{Repo, Run};
use crate::routers::ui::common::{
    UiPageKind, format, is_ui_authenticated, render_page, status_pill, ui_login_redirect_for,
    ui_path,
};
use crate::scheduler::{queue, repos};
use actix_web::{HttpResponse, Responder, get, http::header::ContentType, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use quench_starter::actix::routers::ui::pages::home::handle_home;
use quench_web::prelude::*;
use std::collections::HashMap;

/// How many runs the front page shows. Enough to see what is happening now,
/// short enough that the page does not become a database export.
const RECENT: i64 = 25;

/// Slower than the run page's, and it never stops: this is a dashboard with no
/// resting state to reach, where a run appearing a few seconds late costs
/// nothing. Both panels are whole-state replacements with no stream inside
/// them, so the swap can take the lot.
const POLL_INTERVAL: &str = "every 5s";

#[get("/home")]
pub(super) async fn home(
    request: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    render(request, config, db).await
}

#[get("/home/")]
pub(super) async fn home_slash(
    request: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    render(request, config, db).await
}

async fn render(
    request: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> HttpResponse {
    // Read before the auth check so the closure `handle_home` takes has
    // everything it needs; an unauthenticated caller is redirected either way
    // and the reads are cheap.
    let repositories = repos::list(&db).await.unwrap_or_default();
    let runs = queue::list_runs(&db, None, RECENT)
        .await
        .unwrap_or_default();

    handle_home(request, config, || {
        render_page(
            HttpResponse::Ok(),
            content()
                .class("home-content")
                .child(page(&runs, &repositories)),
            UiPageKind::Home,
        )
    })
    .await
}

fn page(runs: &[Run], repositories: &[Repo]) -> Element {
    div()
        .class("home-container")
        .child(
            div()
                .class("home-header")
                .child(h3().attr("data-i18n", "ui_home_title"))
                .child(
                    p().class("home-subtitle")
                        .attr("data-i18n", "ui_home_subtitle"),
                ),
        )
        .child(sections(runs, repositories))
}

/// Both panels, and the element that asks for them again. Rendered by the page
/// and by the fragment alike, so the swap cannot drift from the first paint.
fn sections(runs: &[Run], repositories: &[Repo]) -> Element {
    div()
        .attr("id", "home-state")
        .class("home-sections")
        .attr("hx-get", ui_path("/home/state"))
        .attr("hx-trigger", POLL_INTERVAL)
        .attr("hx-swap", "outerHTML")
        .child(runs_panel(runs, repositories))
        .child(repos_panel(repositories))
}

/// The polled half of the front page.
#[get("/home/state")]
pub(super) async fn home_state(
    request: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    if !is_ui_authenticated(&request, &config).await {
        return ui_login_redirect_for(&request);
    }

    let repositories = repos::list(&db).await.unwrap_or_default();
    let runs = queue::list_runs(&db, None, RECENT)
        .await
        .unwrap_or_default();

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(sections(&runs, &repositories).render())
}

fn runs_panel(runs: &[Run], repositories: &[Repo]) -> Element {
    let by_id: HashMap<&str, &Repo> = repositories
        .iter()
        .map(|repo| (repo.id.as_str(), repo))
        .collect();

    let panel = div().class("panel").child(
        div()
            .class("panel-title")
            .attr("data-i18n", "ui_runs_title"),
    );

    if runs.is_empty() {
        return panel.child(div().class("empty").attr("data-i18n", "ui_runs_empty"));
    }

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

    panel.child(table)
}

fn repos_panel(repositories: &[Repo]) -> Element {
    let panel = div().class("panel").child(
        div()
            .class("panel-title")
            .attr("data-i18n", "ui_repos_title"),
    );

    if repositories.is_empty() {
        return panel.child(div().class("empty").attr("data-i18n", "ui_repos_empty"));
    }

    let mut table = element("table").class("run-table").child(
        element("tr")
            .child(element("th").attr("data-i18n", "ui_col_repository"))
            .child(element("th").attr("data-i18n", "ui_col_provider"))
            .child(element("th").attr("data-i18n", "ui_col_branch"))
            .child(element("th").attr("data-i18n", "ui_col_state")),
    );

    for repo in repositories {
        table = table.child(
            element("tr")
                .child(
                    element("td").child(
                        a().attr(
                            "href",
                            ui_path(&format!("/repos/{}/{}/scan", repo.owner, repo.name)),
                        )
                        .text(repo.slug()),
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
                })),
        );
    }

    panel.child(table)
}
