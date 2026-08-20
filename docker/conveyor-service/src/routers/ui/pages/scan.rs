//! One repository's code-quality summary: clippy, unused dependencies and
//! known vulnerabilities, read from its most recent run. See `crate::scan`
//! for where the data actually comes from - this file only renders it.
//!
//! Two pages: an overview of cards (one per check, showing a headline and how
//! many findings it has) and, per check, a detail subpage listing every
//! finding with whatever fields the parser could pull out of it (severity,
//! advisory date, file location, ...).

use crate::domain::Repo;
use crate::routers::ui::common::{
    UiPageKind, format, is_ui_authenticated, render_page, ui_login_redirect, ui_path,
};
use crate::scan::{CheckKind, CheckResult, Finding, ScanSummary};
use crate::scheduler::repos;
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;

#[get("/repos/{owner}/{name}/scan")]
pub(super) async fn scan_page(
    request: HttpRequest,
    path: web::Path<(String, String)>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    if !is_ui_authenticated(&request, &config).await {
        return ui_login_redirect();
    }

    let (owner, name) = path.into_inner();
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
        HttpResponse::Ok(),
        content()
            .class("home-content")
            .child(overview(&repo, &summary)),
        UiPageKind::Home,
    )
}

#[get("/repos/{owner}/{name}/scan/{category}")]
pub(super) async fn scan_detail_page(
    request: HttpRequest,
    path: web::Path<(String, String, String)>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    if !is_ui_authenticated(&request, &config).await {
        return ui_login_redirect();
    }

    let (owner, name, category) = path.into_inner();
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
        HttpResponse::Ok(),
        content()
            .class("home-content")
            .child(detail(&repo, kind, check)),
        UiPageKind::Home,
    )
}

async fn load_repo(db: &Db, owner: &str, name: &str) -> Result<Option<Repo>, HttpResponse> {
    repos::find_by_owner_name(db, owner, name)
        .await
        .map_err(|error| {
            tracing::error!("could not read repository {owner}/{name}: {error}");
            HttpResponse::ServiceUnavailable().body(error.to_string())
        })
}

async fn load_summary(
    db: &Db,
    repo: &Repo,
    owner: &str,
    name: &str,
) -> Result<ScanSummary, HttpResponse> {
    crate::scan::latest(db, &repo.id).await.map_err(|error| {
        tracing::error!("could not read scan summary for {owner}/{name}: {error}");
        HttpResponse::ServiceUnavailable().body(error.to_string())
    })
}

fn not_found() -> HttpResponse {
    render_page(
        HttpResponse::NotFound(),
        content().class("home-content").child(
            div()
                .class("home-container")
                .child(empty_state("ui_scan_repo_not_found")),
        ),
        UiPageKind::Home,
    )
}

// ---------------------------------------------------------------------------
// Overview: one card per check
// ---------------------------------------------------------------------------

fn overview(repo: &Repo, summary: &ScanSummary) -> Element {
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

fn overview_body(repo: &Repo, summary: &ScanSummary) -> Element {
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

fn card(repo: &Repo, check: &CheckResult) -> Element {
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

// ---------------------------------------------------------------------------
// Detail: every finding for one check
// ---------------------------------------------------------------------------

fn detail(repo: &Repo, kind: CheckKind, check: &CheckResult) -> Element {
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

/// `unmaintained`/`yanked` read as warnings; anything else (a CVSS-style
/// string, or lint's own `warning`/`error`) is passed through as-is - lint's
/// severities already match the estate's `status-warning`/`status-failed`
/// naming, and an audit CVSS string just falls back to the plain style.
fn severity_class(severity: &str) -> String {
    match severity {
        "warning" | "unmaintained" | "yanked" => {
            "finding-severity finding-severity-warning".to_string()
        }
        "error" => "finding-severity finding-severity-error".to_string(),
        _ => "finding-severity".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Provider, Run, Status, Trigger};
    use chrono::Utc;

    fn repo() -> Repo {
        Repo {
            id: "repo-1".to_string(),
            provider: Provider::GitHub,
            owner: "lorehaven".to_string(),
            name: "palantir".to_string(),
            clone_url: "https://github.com/lorehaven/palantir.git".to_string(),
            default_branch: "main".to_string(),
            registered_by: "admin".to_string(),
            project_id: "project-1".to_string(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn run() -> Run {
        Run {
            id: "run-1".to_string(),
            repo_id: "repo-1".to_string(),
            trigger: Trigger::Push,
            git_ref: "refs/heads/main".to_string(),
            sha: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
            message: Some("a commit".to_string()),
            delivery_id: None,
            status: Status::Success,
            queued_at: Utc::now(),
            started_at: Some(Utc::now()),
            finished_at: Some(Utc::now()),
            claimed_by: None,
            claimed_at: None,
            attempt: 0,
            error: None,
        }
    }

    #[test]
    fn renders_no_runs_yet() {
        let html = overview_body(&repo(), &ScanSummary::default()).render();
        assert!(html.contains("ui_scan_no_runs"));
    }

    #[test]
    fn renders_no_checks_configured() {
        let summary = ScanSummary {
            run: Some(run()),
            ..ScanSummary::default()
        };
        let html = overview_body(&repo(), &summary).render();
        assert!(html.contains("ui_scan_no_checks"));
        assert!(html.contains("deadbee"));
    }

    #[test]
    fn renders_cards_with_counts_and_links() {
        let summary = ScanSummary {
            run: Some(run()),
            lint: Some(CheckResult {
                kind: CheckKind::Lint,
                job_name: "quality/checks".to_string(),
                passed: false,
                headline: "1 warning".to_string(),
                findings: vec![Finding {
                    title: "unused variable: x".to_string(),
                    severity: Some("warning".to_string()),
                    ..Finding::default()
                }],
                metric: None,
            }),
            machete: None,
            audit: None,
            coverage: None,
        };

        let html = overview(&repo(), &summary).render();

        assert!(html.contains("lorehaven/palantir"));
        assert!(html.contains("ui_scan_lint_title"));
        assert!(html.contains(">1<"));
        assert!(html.contains("/ui/repos/lorehaven/palantir/scan/lint"));
        assert!(html.contains("status-failed"));
        // machete/audit/coverage weren't configured - no cards for them at all.
        assert!(!html.contains("ui_scan_machete_title"));
        assert!(!html.contains("ui_scan_audit_title"));
        assert!(!html.contains("ui_scan_coverage_title"));
    }

    #[test]
    fn a_coverage_card_shows_its_metric_not_the_capped_finding_count() {
        let check = CheckResult {
            kind: CheckKind::Coverage,
            job_name: "test/coverage".to_string(),
            passed: true,
            headline: "22.10% line coverage".to_string(),
            // Capped at 50 the same way a real, mostly-uncovered workspace
            // would be - the card must still show the percentage, not "50".
            findings: (0..50)
                .map(|_| Finding {
                    title: "some/file.rs".to_string(),
                    ..Finding::default()
                })
                .collect(),
            metric: Some("22%".to_string()),
        };

        let html = card(&repo(), &check).render();

        assert!(html.contains(">22%<"));
        assert!(!html.contains(">50<"));
    }

    #[test]
    fn renders_finding_detail_fields() {
        let check = CheckResult {
            kind: CheckKind::Audit,
            job_name: "quality/checks".to_string(),
            passed: false,
            headline: "1 finding".to_string(),
            findings: vec![Finding {
                title: "Marvin Attack".to_string(),
                id: Some("RUSTSEC-2023-0071".to_string()),
                severity: Some("5.9 (medium)".to_string()),
                date: Some("2023-11-22".to_string()),
                location: Some("rsa 0.9.10".to_string()),
                extra: Some("Solution: No fixed upgrade is available!".to_string()),
            }],
            metric: None,
        };

        let html = detail(&repo(), CheckKind::Audit, &check).render();

        assert!(html.contains("Marvin Attack"));
        assert!(html.contains("RUSTSEC-2023-0071"));
        assert!(html.contains("5.9 (medium)"));
        assert!(html.contains("2023-11-22"));
        assert!(html.contains("rsa 0.9.10"));
        assert!(html.contains("No fixed upgrade is available!"));
    }

    #[test]
    fn renders_clean_detail_page() {
        let check = CheckResult {
            kind: CheckKind::Machete,
            job_name: "quality/checks".to_string(),
            passed: true,
            headline: "clean".to_string(),
            findings: vec![],
            metric: None,
        };

        let html = detail(&repo(), CheckKind::Machete, &check).render();
        assert!(html.contains("ui_scan_clean"));
    }
}
