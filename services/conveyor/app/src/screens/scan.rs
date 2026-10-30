//! A repository's code-quality summary: an overview card per check, and a page per check with its findings.

use conveyor_core::scan::{LEVELS, chosen_levels, counts, level_class, severity_class, title_key};
use conveyor_core::{CheckDetail, Repo, Run, ScanFindings, format};
use quench_ui::prelude::*;
use quench_ui::router::{Router, link};

use super::{Env, nothing, panel, waiting};
use crate::live::State;

/// The repository and its scan, once both are there; `Err` is what to show instead.
fn load(
    env: &Env,
    state: &State,
    owner: &str,
    name: &str,
) -> Result<(Repo, conveyor_core::ScanReport), View> {
    let repo = state.repos.with(|repos| {
        repos.as_ref().map(|repos| {
            repos
                .iter()
                .find(|r| r.owner == owner && r.name == name)
                .cloned()
        })
    });
    let repo = match repo {
        None => return Err(waiting(env)),
        Some(None) => {
            return Err(nothing(env, "ui_scan_repo_not_found", "not-found").into_view());
        }
        Some(Some(repo)) => repo,
    };
    let report = state.scan.with(|scan| match scan {
        Some((id, report)) if *id == repo.id => Some(report.clone()),
        _ => None,
    });
    report
        .map(|report| (repo, report))
        .ok_or_else(|| waiting(env))
}

/// "abc1234 · schedule · 3 hours ago": which run said this, since each check is the newest of its own kind.
fn origin_text(env: &Env, run: &Run) -> String {
    format!(
        "{} · {} · {}",
        run.short_sha(),
        run.trigger,
        env.since(run.queued_at)
    )
}

// ── Overview ─────────────────────────────────────────────────────────────────

pub(crate) fn scan(env: &Env, state: &State, owner: &str, name: &str) -> View {
    let (env, state, owner, name) = (
        env.clone(),
        state.clone(),
        owner.to_string(),
        name.to_string(),
    );
    let ready = {
        let (state, owner, name, env) = (state.clone(), owner.clone(), name.clone(), env.clone());
        create_memo(move || load(&env, &state, &owner, &name).ok())
    };
    View::dynamic(move || {
        let page = div().attr("id", "scan").class("cv-page");
        let Some((repo, report)) = ready.get() else {
            return match load(&env, &state, &owner, &name) {
                Err(view) => page.child(view).into_view(),
                Ok(_) => View::Empty,
            };
        };
        let launch = link(&env.repo_path(&repo, "/run"))
            .attr("id", "launch-link")
            .class("cv-btn cv-small")
            .text(env.t.t("ui_launch_link"));
        let head = div()
            .class("cv-head")
            .child(h1().text(repo.slug()).child(launch))
            .child(p().class("cv-subtitle").text(env.t.t("ui_scan_subtitle")));
        page.child(head)
            .child(overview_body(&env, &repo, &report))
            .into_view()
    })
}

fn overview_body(env: &Env, repo: &Repo, report: &conveyor_core::ScanReport) -> Element {
    let Some(run) = &report.run else {
        return panel(div()).child(nothing(env, "ui_scan_no_runs", "no-runs"));
    };
    let meta = div()
        .attr("id", "scan-run")
        .class("cv-run-meta")
        .child(
            link(&env.path(&format!("/runs/{}", run.id)))
                .class("cv-mono")
                .text(run.short_sha().to_string()),
        )
        .child(span().class("cv-muted").text(run.ref_name().to_string()))
        .child(span().class("cv-muted").text(env.since(run.queued_at)));
    let mut out = div().child(meta);
    if report.checks.is_empty() {
        return out.child(panel(div()).child(nothing(env, "ui_scan_no_checks", "no-checks")));
    }
    let mut grid = div().attr("id", "scan-grid").class("cv-scan-grid");
    for check in &report.checks {
        grid = grid.child(card(env, repo, check));
    }
    out = out.child(grid);
    out
}

fn card(env: &Env, repo: &Repo, check: &CheckDetail) -> Element {
    let state = if check.passed {
        "cv-status-success"
    } else {
        "cv-status-failed"
    };
    link(&env.repo_path(repo, &format!("/scan/{}", check.kind)))
        .class(format!("cv-scan-card {state}"))
        .attr("data-check", check.kind.clone())
        .child(
            div().class("cv-scan-count").text(
                check
                    .metric
                    .clone()
                    .unwrap_or_else(|| check.findings.len().to_string()),
            ),
        )
        .child(
            div()
                .class("cv-scan-title")
                .text(env.t.t(title_key(&check.kind).unwrap_or("ui_not_found"))),
        )
        .child(div().class("cv-scan-headline").text(check.headline.clone()))
        .child(check.origin.as_ref().map(|run| {
            div()
                .class("cv-scan-origin cv-muted")
                .text(origin_text(env, run))
        }))
}

// ── One check ────────────────────────────────────────────────────────────────

pub(crate) fn check(
    env: &Env,
    state: &State,
    _router: &Router,
    owner: &str,
    name: &str,
    kind: &str,
) -> View {
    let (env, state, owner, name, kind) = (
        env.clone(),
        state.clone(),
        owner.to_string(),
        name.to_string(),
        kind.to_string(),
    );
    let ready = {
        let (state, owner, name, env) = (state.clone(), owner.clone(), name.clone(), env.clone());
        create_memo(move || load(&env, &state, &owner, &name).ok())
    };
    View::dynamic(move || {
        let page = div().attr("id", "check").class("cv-page");
        let Some((repo, report)) = ready.get() else {
            return match load(&env, &state, &owner, &name) {
                Err(view) => page.child(view).into_view(),
                Ok(_) => View::Empty,
            };
        };
        let Some(detail) = report.check(&kind).cloned() else {
            return page
                .child(nothing(&env, "ui_scan_repo_not_found", "not-found"))
                .into_view();
        };
        check_page(&env, &state, page, &repo, &detail)
    })
}

fn check_page(env: &Env, state: &State, page: Element, repo: &Repo, check: &CheckDetail) -> View {
    let subtitle = check.origin.as_ref().map_or_else(
        || format!("{} - from {}", check.headline, check.job_name),
        |run| {
            format!(
                "{} - from {} ({})",
                check.headline,
                check.job_name,
                origin_text(env, run)
            )
        },
    );
    let head = div()
        .class("cv-head")
        .child(
            link(&env.repo_path(repo, "/scan"))
                .attr("id", "back")
                .class("cv-mono cv-muted")
                .text(env.t.t("ui_scan_back")),
        )
        .child(h3().text(env.t.t(title_key(&check.kind).unwrap_or("ui_not_found"))))
        .child(
            p().attr("id", "check-subtitle")
                .class("cv-subtitle")
                .text(subtitle),
        );
    let mut page = page.child(head);
    if check.has_report {
        page = page.child(report_viewer(env, repo, &check.kind));
    }
    page = page.child(findings_list(env, check));
    if check.kind == "security" {
        page = page.child(scanner_panel(env, state, repo));
    }
    page.into_view()
}

fn report_url(env: &Env, repo: &Repo, kind: &str) -> String {
    format!("{}/api/v1/repos/{}/scan/{kind}/report", env.base, repo.id)
}

/// The stored report, right on the page, with buttons to open it on its own and to save it. Sandboxed: it can filter itself but
/// never reach the service's session.
fn report_viewer(env: &Env, repo: &Repo, kind: &str) -> Element {
    let url = report_url(env, repo, kind);
    div()
        .attr("id", "report")
        .class("cv-report")
        .child(
            div()
                .class("cv-log-toolbar")
                .child(
                    a().attr("href", url.clone())
                        .attr("target", "_blank")
                        .attr("rel", "noopener")
                        .attr("id", "report-open")
                        .attr("title", env.t.t("ui_scan_tests_report_open"))
                        .class("cv-log-action")
                        .text("↗"),
                )
                .child(
                    a().attr("href", format!("{url}?download=1"))
                        .attr("download", "")
                        .attr("id", "report-download")
                        .attr("title", env.t.t("ui_scan_tests_report_download"))
                        .class("cv-log-action")
                        .text("⤓"),
                ),
        )
        .child(
            element("iframe")
                .attr("id", "report-frame")
                .class("cv-report-frame")
                .attr("src", url)
                .attr("sandbox", "allow-scripts")
                .attr("loading", "lazy")
                .attr("title", "Report"),
        )
}

fn findings_list(env: &Env, check: &CheckDetail) -> Element {
    if check.findings.is_empty() {
        return panel(div()).child(nothing(env, "ui_scan_clean", "clean"));
    }
    let mut list = div().attr("id", "findings").class("cv-findings");
    for finding in &check.findings {
        let mut head = div()
            .class("cv-finding-head")
            .child(div().text(finding.title.clone()));
        head = head.child(
            finding
                .severity
                .as_ref()
                .map(|s| span().class(severity_class(s)).text(s.clone())),
        );
        let meta = div()
            .class("cv-finding-meta")
            .child(
                finding
                    .id
                    .as_ref()
                    .map(|id| span().class("cv-mono").text(id.clone())),
            )
            .child(
                finding
                    .date
                    .as_ref()
                    .map(|d| span().class("cv-muted").text(d.clone())),
            )
            .child(
                finding
                    .location
                    .as_ref()
                    .map(|l| span().class("cv-mono cv-muted").text(l.clone())),
            );
        list = list.child(
            div().class("cv-finding").child(head).child(meta).child(
                finding
                    .extra
                    .as_ref()
                    .map(|e| div().class("cv-finding-extra").text(e.clone())),
            ),
        );
    }
    list
}

// ── The scanner's findings ───────────────────────────────────────────────────

fn scanner_panel(env: &Env, state: &State, repo: &Repo) -> View {
    let (env, findings, id) = (env.clone(), state.findings.clone(), repo.id.clone());
    let found = create_memo(move || {
        findings.with(|f| match f {
            Some((repo, Some(found))) if *repo == id => Some(found.clone()),
            _ => None,
        })
    });
    View::dynamic(move || {
        found
            .get()
            .map_or(View::Empty, |found| scanner_findings(&env, &found))
    })
}

/// Every finding of the latest scan at the chosen levels, with a box per level to change them. Until a filter is applied every
/// level shows, whatever the boxes say.
fn scanner_findings(env: &Env, found: &ScanFindings) -> View {
    let tally = counts(&found.findings);
    let ticks: Vec<Signal<bool>> = LEVELS.iter().map(|_| create_signal(true)).collect();
    let applied = create_signal(false);
    let chosen: Signal<Vec<&'static str>> = create_signal(LEVELS.to_vec());

    let apply = {
        let (ticks, applied, chosen) = (ticks.clone(), applied.clone(), chosen.clone());
        move || {
            let on: Vec<&str> = LEVELS
                .iter()
                .zip(&ticks)
                .filter(|(_, tick)| tick.get_untracked())
                .map(|(level, _)| *level)
                .collect();
            applied.set(true);
            chosen.set(chosen_levels(&on, true));
        }
    };
    let mut filter = form()
        .attr("id", "finding-filter")
        .class("cv-filter")
        .on_submit(apply);
    for ((level, count), tick) in LEVELS.iter().zip(tally).zip(&ticks) {
        let id = format!("level-{level}");
        let (on, toggle) = (tick.clone(), tick.clone());
        filter = filter.child(
            div()
                .class("cv-filter-level")
                .child(
                    input()
                        .attr("type", "checkbox")
                        .attr("id", id.clone())
                        .attr("name", "level")
                        .attr_bool("checked", move || on.get())
                        .on_click(move || toggle.update(|v| *v = !*v)),
                )
                .child(
                    label().attr("for", id).child(
                        span()
                            .class(level_class(level))
                            .text(format!("{level} ({count})")),
                    ),
                ),
        );
    }
    filter = filter.child(
        button()
            .attr("type", "submit")
            .attr("id", "apply-filter")
            .class("cv-btn")
            .text(env.t.t("ui_findings_apply")),
    );

    let total = found.findings.len();
    let stamp = format::stamp(found.created_at);
    let list = {
        let (env, found, chosen) = (env.clone(), found.clone(), chosen.clone());
        View::dynamic(move || {
            let levels = chosen.get();
            let shown: Vec<&conveyor_core::ScanFinding> = found
                .findings
                .iter()
                .filter(|f| levels.contains(&f.severity.as_str()))
                .collect();
            let title = span()
                .attr("id", "findings-count")
                .class("cv-muted")
                .text(format!(" - {} / {total} - {stamp}", shown.len()));
            let body = if shown.is_empty() {
                panel(div())
                    .child(nothing(&env, "ui_findings_none", "no-findings"))
                    .into_view()
            } else {
                let mut list = div().attr("id", "scanner-list").class("cv-findings");
                for finding in shown {
                    let route = if finding.method == "-" || finding.method.is_empty() {
                        finding.service.clone()
                    } else {
                        format!("{} {} {}", finding.service, finding.method, finding.path)
                    };
                    list = list.child(
                        div()
                            .class("cv-finding")
                            .child(
                                div()
                                    .class("cv-finding-head")
                                    .child(div().text(route))
                                    .child(
                                        span()
                                            .class(level_class(&finding.severity))
                                            .text(finding.severity.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .class("cv-finding-meta")
                                    .child(span().class("cv-mono").text(finding.check.clone())),
                            )
                            .child(div().class("cv-finding-extra").text(finding.detail.clone())),
                    );
                }
                list.into_view()
            };
            div()
                .child(div().class("cv-muted").child(title))
                .child(body)
                .into_view()
        })
    };
    panel(div().child(span().text(env.t.t("ui_findings_title"))))
        .attr("id", "scanner-findings")
        .child(filter)
        .child(list)
        .into_view()
}
