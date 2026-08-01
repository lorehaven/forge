//! One repository's code-quality summary: clippy, unused dependencies and
//! known vulnerabilities, read from its most recent run. See `crate::scan`
//! for where the data actually comes from - this file only renders it.

use crate::domain::Repo;
use crate::routers::ui::common::{
    UiPageKind, format, is_ui_authenticated, render_page, ui_login_redirect, ui_path,
};
use crate::scan::{CheckResult, ScanSummary};
use crate::scheduler::repos;
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use quench_web::prelude::*;

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
    let repo = match repos::find_by_owner_name(&db, &owner, &name).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return not_found(),
        Err(error) => {
            tracing::error!("could not read repository {owner}/{name}: {error}");
            return HttpResponse::ServiceUnavailable().body(error.to_string());
        }
    };

    let summary = match crate::scan::latest(&db, &repo.id).await {
        Ok(summary) => summary,
        Err(error) => {
            tracing::error!("could not read scan summary for {owner}/{name}: {error}");
            return HttpResponse::ServiceUnavailable().body(error.to_string());
        }
    };

    render_page(
        HttpResponse::Ok(),
        content().class("home-content").child(page(&repo, &summary)),
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
                    .attr("data-i18n", "ui_scan_repo_not_found"),
            ),
        ),
        UiPageKind::Home,
    )
}

fn page(repo: &Repo, summary: &ScanSummary) -> Element {
    div()
        .class("home-container")
        .child(header_row(repo))
        .child(body(summary))
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

fn body(summary: &ScanSummary) -> Element {
    let Some(run) = &summary.run else {
        return div()
            .class("panel")
            .child(div().class("empty").attr("data-i18n", "ui_scan_no_runs"));
    };

    let mut container = div().child(run_row(run));

    if summary.is_empty() {
        return container.child(
            div()
                .class("panel")
                .child(div().class("empty").attr("data-i18n", "ui_scan_no_checks")),
        );
    }

    let mut grid = div().class("scan-grid");
    grid = grid.child_opt(summary.lint.as_ref().map(check_panel));
    grid = grid.child_opt(summary.machete.as_ref().map(check_panel));
    grid = grid.child_opt(summary.audit.as_ref().map(check_panel));
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

fn check_panel(check: &CheckResult) -> Element {
    let title_key = check.kind.label();
    let status_class = if check.passed {
        "status-success"
    } else {
        "status-failed"
    };

    let mut panel = div()
        .class("panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", title_key)
                .child(
                    span()
                        .class(format!("status {status_class}"))
                        .text(&check.headline),
                ),
        )
        .child(
            span()
                .class("muted")
                .text(format!("from {}", check.job_name)),
        );

    if !check.details.is_empty() {
        let mut list = element("ul").class("scan-details");
        for line in &check.details {
            list = list.child(element("li").class("mono").text(line));
        }
        panel = panel.child(list);
    }

    panel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Provider, Run, Status, Trigger};
    use crate::scan::CheckKind;
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
        let html = body(&ScanSummary::default()).render();
        assert!(html.contains("ui_scan_no_runs"));
    }

    #[test]
    fn renders_no_checks_configured() {
        let summary = ScanSummary {
            run: Some(run()),
            ..ScanSummary::default()
        };
        let html = body(&summary).render();
        assert!(html.contains("ui_scan_no_checks"));
        // Still shows the run itself, even with nothing to check.
        assert!(html.contains("deadbee"));
    }

    #[test]
    fn renders_a_full_summary() {
        let summary = ScanSummary {
            run: Some(run()),
            lint: Some(CheckResult {
                kind: CheckKind::Lint,
                job_name: "quality/checks".to_string(),
                passed: false,
                headline: "1 warning, 1 error".to_string(),
                details: vec!["warning: unused variable: x".to_string()],
            }),
            machete: Some(CheckResult {
                kind: CheckKind::Machete,
                job_name: "quality/checks".to_string(),
                passed: true,
                headline: "clean".to_string(),
                details: vec![],
            }),
            audit: None,
        };

        let html = page(&repo(), &summary).render();

        assert!(html.contains("lorehaven/palantir"));
        assert!(html.contains("ui_scan_lint_title"));
        assert!(html.contains("1 warning, 1 error"));
        assert!(html.contains("status-failed"));
        assert!(html.contains("ui_scan_machete_title"));
        assert!(html.contains("clean"));
        assert!(html.contains("status-success"));
        // No audit result was set - its panel must not appear at all.
        assert!(!html.contains("ui_scan_audit_title"));
    }
}
