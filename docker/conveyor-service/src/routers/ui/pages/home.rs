//! Conveyor's front page: what has run lately, and what is registered.

use crate::domain::{Repo, Run};
use crate::routers::ui::common::{
    UiPageKind, format, is_ui_authenticated, render_page, status_pill, ui_login_redirect_for,
    ui_path,
};
use crate::scan::{CheckKind, CheckResult, ScanSummary};
use crate::scheduler::{queue, repos};
use actix_web::{HttpResponse, Responder, get, http::header::ContentType, post, web};
use futures_util::future::join_all;
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
    let scans = scan_summaries(&db, &repositories).await;

    handle_home(request, config, || {
        render_page(
            HttpResponse::Ok(),
            content()
                .class("home-content")
                .child(page(&runs, &repositories, &scans)),
            UiPageKind::Home,
        )
    })
    .await
}

/// One lookup per repository, run concurrently - `scan::latest` is several
/// sequential DB round trips on its own, and the front page has no other use
/// for waiting on them one repository at a time.
async fn scan_summaries(db: &Db, repositories: &[Repo]) -> HashMap<String, ScanSummary> {
    let fetches = repositories.iter().map(|repo| async move {
        let summary = crate::scan::latest(db, &repo.id).await.unwrap_or_default();
        (repo.id.clone(), summary)
    });
    join_all(fetches).await.into_iter().collect()
}

fn page(runs: &[Run], repositories: &[Repo], scans: &HashMap<String, ScanSummary>) -> Element {
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
        .child(sections(runs, repositories, scans))
}

/// Both panels, and the element that asks for them again. Rendered by the page
/// and by the fragment alike, so the swap cannot drift from the first paint.
fn sections(runs: &[Run], repositories: &[Repo], scans: &HashMap<String, ScanSummary>) -> Element {
    div()
        .attr("id", "home-state")
        .class("home-sections")
        .attr("hx-get", ui_path("/home/state"))
        .attr("hx-trigger", POLL_INTERVAL)
        .attr("hx-swap", "outerHTML")
        .child(runs_panel(runs, repositories))
        .child(repos_panel(repositories, scans))
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
    let scans = scan_summaries(&db, &repositories).await;

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(sections(&runs, &repositories, &scans).render())
}

/// Starts a run for one repository by hand, then hands back the same fragment
/// `/home/state` polls - the button's own request settles the table it sits
/// in, rather than waiting for the next poll to notice.
#[post("/home/repos/{repo_id}/run")]
pub(super) async fn run_now(
    request: actix_web::HttpRequest,
    path: web::Path<String>,
    config: web::Data<JwtConfig>,
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
    let runs = queue::list_runs(&db, None, RECENT)
        .await
        .unwrap_or_default();
    let scans = scan_summaries(&db, &repositories).await;

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(sections(&runs, &repositories, &scans).render())
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

fn repos_panel(repositories: &[Repo], scans: &HashMap<String, ScanSummary>) -> Element {
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
            .child(element("th").attr("data-i18n", "ui_col_state"))
            .child(element("th").attr("data-i18n", "ui_col_checks"))
            .child(element("th").attr("data-i18n", "ui_col_actions")),
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
                }))
                .child(element("td").child(check_chips(scans.get(&repo.id))))
                .child(element("td").child(run_button(repo))),
        );
    }

    panel.child(table)
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
pub fn run_button(repo: &Repo) -> Element {
    if !repo.enabled {
        return span();
    }

    button()
        .attr("type", "button")
        .class("run-button")
        .attr("data-i18n", "ui_repo_run_now")
        .attr("hx-post", ui_path(&format!("/home/repos/{}/run", repo.id)))
        .attr("hx-target", "#home-state")
        .attr("hx-swap", "outerHTML")
}
