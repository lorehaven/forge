//! Renders a repo's code-quality summary (`crate::scan` owns the data): an
//! overview card per check, and a detail subpage listing that check's findings.

use crate::domain::Repo;
use crate::routers::ui::common::{PageAuth, format, render_page, ui_login_redirect, ui_path};
use crate::scan::{CheckKind, CheckResult, Finding, ScanSummary};
use crate::scheduler::repos;
use quench_core::bytes::Bytes;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode};
use quench_serde::Deserialize;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;

#[get("/ui/repos/{owner}/{name}/scan")]
pub(super) async fn scan_page(
    PageAuth(authenticated): PageAuth,
    Path((owner, name)): Path<(String, String)>,
    Inject(db): Inject<Db>,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }

    let repo = match load_repo(&db, &owner, &name).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };

    let summary = match load_summary(&db, &repo, &owner, &name).await {
        Ok(summary) => summary,
        Err(response) => return response,
    };

    render_page(
        StatusCode::OK,
        content()
            .class("home-content")
            .child(overview(&repo, &summary)),
    )
}

#[get("/ui/repos/{owner}/{name}/scan/{category}")]
pub(super) async fn scan_detail_page(
    PageAuth(authenticated): PageAuth,
    Path((owner, name, category)): Path<(String, String, String)>,
    Inject(db): Inject<Db>,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }

    let Some(kind) = CheckKind::from_slug(&category) else {
        return not_found();
    };

    let repo = match load_repo(&db, &owner, &name).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };

    let summary = match load_summary(&db, &repo, &owner, &name).await {
        Ok(summary) => summary,
        Err(response) => return response,
    };

    let Some(check) = summary.get(kind) else {
        return not_found();
    };

    render_page(
        StatusCode::OK,
        content()
            .class("home-content")
            .child(detail(&repo, kind, check)),
    )
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ReportQuery {
    /// Any value: hand the page over as a file to save, instead of showing it.
    download: Option<String>,
}

/// The stored test report page, served by conveyor: warehouse hands files out as downloads, and it needs the
/// reader to hold a warehouse grant. Sandboxed (`allow-scripts` only, no origin of its own) so a report can
/// filter itself but never reach conveyor's session.
#[get("/ui/repos/{owner}/{name}/scan/tests/report")]
pub(super) async fn test_report(
    PageAuth(authenticated): PageAuth,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<ReportQuery>,
    Inject(db): Inject<Db>,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }
    let repo = match load_repo(&db, &owner, &name).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return not_found(),
        Err(response) => return response,
    };
    let summary = match load_summary(&db, &repo, &owner, &name).await {
        Ok(summary) => summary,
        Err(response) => return response,
    };
    let (Some(uri), Some(store)) = (
        summary.tests.and_then(|check| check.report_uri),
        crate::scan::warehouse_store(),
    ) else {
        return not_found();
    };

    let page = match store.download(&uri).await {
        Ok(page) => page,
        Err(error) => {
            quench_log::warn!("could not read the test report of {owner}/{name}: {error}");
            return Response::text(StatusCode::BAD_GATEWAY, "the report could not be read");
        }
    };

    let response = Response::from_bytes(StatusCode::OK, Bytes::from(page))
        .header("content-type", "text/html; charset=utf-8")
        .header("content-security-policy", "sandbox allow-scripts")
        .header("x-content-type-options", "nosniff")
        .header("cache-control", "private, max-age=60");
    if query.download.is_some() {
        response.header(
            "content-disposition",
            format!("attachment; filename=\"{owner}-{name}-test-report.html\""),
        )
    } else {
        response
    }
}

async fn load_repo(db: &Db, owner: &str, name: &str) -> Result<Option<Repo>, Response> {
    repos::find_by_owner_name(db, owner, name)
        .await
        .map_err(|error| {
            quench_log::error!("could not read repository {owner}/{name}: {error}");
            Response::text(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
        })
}

async fn load_summary(
    db: &Db,
    repo: &Repo,
    owner: &str,
    name: &str,
) -> Result<ScanSummary, Response> {
    crate::scan::latest(db, &repo.id).await.map_err(|error| {
        quench_log::error!("could not read scan summary for {owner}/{name}: {error}");
        Response::text(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
    })
}

fn not_found() -> Response {
    render_page(
        StatusCode::NOT_FOUND,
        content().class("home-content").child(
            div()
                .class("home-container")
                .child(empty_state("ui_scan_repo_not_found")),
        ),
    )
}

// Overview: one card per check.
#[must_use]
pub fn overview(repo: &Repo, summary: &ScanSummary) -> Element {
    div()
        .class("home-container")
        .child(header_row(repo))
        .child(overview_body(repo, summary))
}

fn header_row(repo: &Repo) -> Element {
    div()
        .class("home-header")
        .child(h3().text(repo.slug()))
        .child(
            p().class("home-subtitle")
                .attr("data-i18n", "ui_scan_subtitle"),
        )
}

#[must_use]
pub fn overview_body(repo: &Repo, summary: &ScanSummary) -> Element {
    let Some(run) = &summary.run else {
        return div().class("panel").child(empty_state("ui_scan_no_runs"));
    };

    let mut container = div().child(run_row(run));

    if summary.is_empty() {
        return container.child(div().class("panel").child(empty_state("ui_scan_no_checks")));
    }

    let mut grid = div().class("scan-grid");
    grid = grid.child_opt(summary.lint.as_ref().map(|c| card(repo, c)));
    grid = grid.child_opt(summary.machete.as_ref().map(|c| card(repo, c)));
    grid = grid.child_opt(summary.audit.as_ref().map(|c| card(repo, c)));
    grid = grid.child_opt(summary.coverage.as_ref().map(|c| card(repo, c)));
    grid = grid.child_opt(summary.tests.as_ref().map(|c| card(repo, c)));
    container = container.child(grid);

    container
}

fn run_row(run: &crate::domain::Run) -> Element {
    div()
        .class("run-meta")
        .child(
            a().attr("href", ui_path(&format!("/runs/{}", run.id)))
                .class("mono")
                .text(run.short_sha()),
        )
        .child(span().class("muted").text(run.ref_name()))
        .child(span().class("muted").text(format::relative(run.queued_at)))
}

#[must_use]
pub fn card(repo: &Repo, check: &CheckResult) -> Element {
    let status_class = if check.passed {
        "status-success"
    } else {
        "status-failed"
    };
    let href = ui_path(&format!(
        "/repos/{}/{}/scan/{}",
        repo.owner,
        repo.name,
        check.kind.slug()
    ));

    a().class("scan-card")
        .class(status_class)
        .attr("href", href)
        .child(
            div().class("scan-card-count").text(
                check
                    .metric
                    .clone()
                    .unwrap_or_else(|| check.findings.len().to_string()),
            ),
        )
        .child(
            div()
                .class("scan-card-title")
                .attr("data-i18n", check.kind.label()),
        )
        .child(div().class("scan-card-headline").text(&check.headline))
}

// Detail: every finding for one check.
#[must_use]
pub fn detail(repo: &Repo, kind: CheckKind, check: &CheckResult) -> Element {
    let back_href = ui_path(&format!("/repos/{}/{}/scan", repo.owner, repo.name));

    div()
        .class("home-container")
        .child(
            div()
                .class("home-header")
                .child(
                    a().attr("href", back_href)
                        .class("mono muted")
                        .attr("data-i18n", "ui_scan_back"),
                )
                .child(h3().attr("data-i18n", kind.label()))
                .child(
                    p().class("home-subtitle")
                        .text(format!("{} - from {}", check.headline, check.job_name)),
                ),
        )
        .child_opt(check.report_uri.as_ref().map(|_| report_viewer(repo)))
        .child(findings_list(&check.findings))
}

fn findings_list(findings: &[Finding]) -> Element {
    if findings.is_empty() {
        return div().class("panel").child(empty_state("ui_scan_clean"));
    }

    let mut list = div().class("finding-list");
    for finding in findings {
        list = list.child(finding_row(finding));
    }
    list
}

fn finding_row(finding: &Finding) -> Element {
    let mut row = div().class("finding-item");

    let mut head = div().class("finding-head");
    head = head.child(div().class("finding-title").text(&finding.title));
    head = head.child_opt(
        finding
            .severity
            .as_ref()
            .map(|severity| span().class(severity_class(severity)).text(severity)),
    );
    row = row.child(head);

    let mut meta = div().class("finding-meta");
    meta = meta.child_opt(finding.id.as_ref().map(|id| span().class("mono").text(id)));
    meta = meta.child_opt(
        finding
            .date
            .as_ref()
            .map(|date| span().class("muted").text(date)),
    );
    meta = meta.child_opt(
        finding
            .location
            .as_ref()
            .map(|location| span().class("mono muted").text(location)),
    );
    row = row.child(meta);

    row = row.child_opt(
        finding
            .extra
            .as_ref()
            .map(|extra| div().class("finding-extra").text(extra)),
    );

    row
}

/// `unmaintained`/`yanked` read as warnings; other strings pass through as-is.
fn severity_class(severity: &str) -> String {
    match severity {
        "warning" | "unmaintained" | "yanked" => {
            "finding-severity finding-severity-warning".to_string()
        }
        "error" => "finding-severity finding-severity-error".to_string(),
        _ => "finding-severity".to_string(),
    }
}

pub(super) fn register_routes() {
    let _ = scan_page as fn(_, _, _) -> _;
    let _ = scan_detail_page as fn(_, _, _) -> _;
    let _ = test_report as fn(_, _, _, _) -> _;
}

/// The report, right on the page, with buttons to open it on its own and to save it.
fn report_viewer(repo: &Repo) -> Element {
    let report = ui_path(&format!(
        "/repos/{}/{}/scan/tests/report",
        repo.owner, repo.name
    ));

    div()
        .class("report-viewer")
        .child(
            div()
                .class("log-toolbar")
                .child(
                    a().class("log-action")
                        .attr("href", &report)
                        .attr("target", "_blank")
                        .attr("rel", "noopener")
                        .attr("title", "Open the full report")
                        .attr("data-i18n-title", "ui_scan_tests_report_open")
                        .child(i().class("fas").class("fa-up-right-from-square")),
                )
                .child(
                    a().class("log-action")
                        .attr("href", format!("{report}?download=1"))
                        .attr("download", "")
                        .attr("title", "Download the report")
                        .attr("data-i18n-title", "ui_scan_tests_report_download")
                        .child(i().class("fas").class("fa-download")),
                ),
        )
        .child(
            element("iframe")
                .class("report-frame")
                .attr("src", report)
                .attr("sandbox", "allow-scripts")
                .attr("loading", "lazy")
                .attr("title", "Test report"),
        )
}
