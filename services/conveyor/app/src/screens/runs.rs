//! The full history, and one run with its jobs, logs and artifacts.

use std::rc::Rc;

use conveyor_core::graph::stage_levels;
use conveyor_core::{Job, Status, format};
use quench_ui::prelude::*;
use quench_ui::router::{Router, link};

use super::{Env, breadcrumb, is_missing, nothing, panel, pill, runs_table, waiting};
use crate::live::{JobLog, Request, State};

// ── The history ──────────────────────────────────────────────────────────────

pub(crate) fn history(env: &Env, state: &State, router: &Router) -> View {
    let (env, state, router) = (env.clone(), state.clone(), router.clone());
    // The project and the page are in the address; the page is rebuilt when they change.
    let scope = router.location().query_get("project");
    let missing = {
        let problem = state.problem.clone();
        create_memo(move || is_missing(&problem))
    };
    View::dynamic(move || {
        if missing.get() {
            return div()
                .class("cv-page")
                .child(nothing(&env, "ui_project_not_found", "not-found"))
                .into_view();
        }
        div()
            .attr("id", "history")
            .class("cv-page")
            .child(history_head(&env, &state, scope.clone()))
            .child(history_panel(&env, &state, scope.clone()))
            .into_view()
    })
}

fn history_head(env: &Env, state: &State, scope: Option<String>) -> View {
    let (env, projects) = (env.clone(), state.projects.clone());
    View::dynamic(move || {
        let title = scope
            .as_ref()
            .and_then(|id| {
                projects.with(|all| {
                    let all = all.as_deref().unwrap_or_default();
                    all.iter()
                        .find(|p| &p.id == id)
                        .map(|project| breadcrumb(&env, all, project))
                })
            })
            .unwrap_or_else(|| h3().text(env.t.t("ui_pipelines_title")));
        div()
            .class("cv-head")
            .child(title)
            .child(
                p().class("cv-subtitle")
                    .text(env.t.t("ui_pipelines_subtitle")),
            )
            .into_view()
    })
}

fn page_href(env: &Env, scope: Option<&String>, page: u32) -> String {
    scope.map_or_else(
        || env.path(&format!("/runs?page={page}")),
        |id| env.path(&format!("/runs?project={id}&page={page}")),
    )
}

fn pager(env: &Env, scope: Option<&String>, page: u32, pages: u32) -> Element {
    let end = |target: Option<u32>, key: &str, id: &str| match target {
        Some(page) => link(&page_href(env, scope, page))
            .attr("id", id.to_string())
            .class("cv-pager-link")
            .text(env.t.t(key)),
        None => span()
            .attr("id", id.to_string())
            .class("cv-pager-link cv-pager-disabled")
            .text(env.t.t(key)),
    };
    div()
        .class("cv-pager")
        .child(end((page > 1).then(|| page - 1), "ui_pager_prev", "prev"))
        .child(
            span()
                .attr("id", "pager-status")
                .class("cv-pager-status")
                .text(env.tr(
                    "ui_pager_page",
                    &[("page", page.into()), ("total", pages.into())],
                )),
        )
        .child(end(
            (page < pages).then_some(page + 1),
            "ui_pager_next",
            "next",
        ))
}

fn history_panel(env: &Env, state: &State, scope: Option<String>) -> View {
    let (env, page, repos) = (env.clone(), state.history.clone(), state.repos.clone());
    View::dynamic(move || {
        let body = page.with(|page| {
            page.as_ref().map(|page| {
                let table = repos.with(|repos| {
                    runs_table(&env, &page.runs, repos.as_deref().unwrap_or_default())
                });
                div()
                    .child(table)
                    .child(pager(&env, scope.as_ref(), page.page, page.pages))
                    .into_view()
            })
        });
        panel(div().text(env.t.t("ui_pipelines_title")))
            .child(body.unwrap_or_else(|| waiting(&env)))
            .into_view()
    })
}

// ── One run ──────────────────────────────────────────────────────────────────

pub(crate) fn run(env: &Env, state: &State, id: String) -> View {
    let missing = {
        let problem = state.problem.clone();
        create_memo(move || is_missing(&problem))
    };
    let loaded = {
        let run = state.run.clone();
        create_memo(move || run.with(|r| r.is_some()))
    };
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        if missing.get() {
            return div()
                .class("cv-page")
                .child(nothing(&env, "ui_run_not_found", "not-found"))
                .into_view();
        }
        if !loaded.get() {
            return div().class("cv-page").child(waiting(&env)).into_view();
        }
        div()
            .attr("id", "run")
            .class("cv-page")
            .child(summary(&env, &state, &id))
            .child(jobs_graph(&env, &state))
            .child(artifacts(&env, &state))
            .into_view()
    })
}

fn labelled_value(env: &Env, key: &str, value: &str) -> Element {
    span()
        .child(span().class("cv-muted").text(env.t.t(key)))
        .child(span().text(format!(" {value}")))
}

/// The header, the facts, the reason and the restart button: all of it follows the polled run.
fn summary(env: &Env, state: &State, id: &str) -> View {
    let (env, run, id) = (env.clone(), state.run.clone(), id.to_string());
    View::dynamic(move || {
        let Some(view) = run.get() else {
            return View::Empty;
        };
        let r = &view.run;
        let now = (env.now)();
        let slug = view
            .repo
            .as_ref()
            .map_or_else(|| env.t.t("ui_unknown_repo"), conveyor_core::Repo::slug);
        let header = div()
            .attr("id", "run-header")
            .class("cv-run-header")
            .child(pill(&env, r.status))
            .child(h3().text(slug))
            .child(
                span()
                    .class("cv-mono cv-muted")
                    .text(r.ref_name().to_string()),
            )
            .child(
                span()
                    .class("cv-mono cv-muted")
                    .text(r.short_sha().to_string()),
            )
            .child(r.message.clone().map(|m| span().class("cv-muted").text(m)));
        let meta = div()
            .attr("id", "run-meta")
            .class("cv-run-meta")
            .child(labelled_value(
                &env,
                "ui_meta_trigger",
                &r.trigger.to_string(),
            ))
            .child(
                r.stages
                    .as_ref()
                    .map(|s| labelled_value(&env, "ui_meta_stages", &s.join(", "))),
            )
            .child(labelled_value(
                &env,
                "ui_meta_queued",
                &env.since(r.queued_at),
            ))
            .child(labelled_value(
                &env,
                "ui_meta_duration",
                &format::elapsed(r.started_at, r.finished_at, now),
            ))
            .child(labelled_value(
                &env,
                "ui_meta_attempt",
                &r.attempt.to_string(),
            ));
        let reason = r.error.clone().map(|error| {
            panel(div().text(env.t.t("ui_run_reason")))
                .attr("id", "run-reason")
                .child(div().class("cv-job-reason").text(error))
        });
        // A restart does not rebuild passed stages.
        let restart = r.status.is_failure().then(|| {
            let (ask, id) = (Rc::clone(&env.ask), id.clone());
            button()
                .attr("type", "button")
                .attr("id", "restart")
                .class("cv-btn cv-run-button")
                .text(env.t.t("ui_run_restart"))
                .on_click(move || ask(Request::Restart(id.clone())))
        });
        div()
            .child(header)
            .child(meta)
            .child(reason)
            .child(restart)
            .into_view()
    })
}

/// A job with only what decides where it sits: the graph is rebuilt when that changes, not when a status does.
fn layout_of(jobs: &[Job]) -> Vec<Job> {
    jobs.iter()
        .map(|j| Job {
            status: Status::Queued,
            exit_code: None,
            started_at: None,
            finished_at: None,
            error: None,
            reused_from_run: None,
            ..j.clone()
        })
        .collect()
}

/// One row per dependency level, laid out the way the worker runs them.
fn jobs_graph(env: &Env, state: &State) -> View {
    let layout = {
        let run = state.run.clone();
        create_memo(move || {
            run.with(|r| r.as_ref().map(|v| layout_of(&v.jobs)).unwrap_or_default())
        })
    };
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let jobs = layout.get();
        let mut graph = div().attr("id", "run-jobs").class("cv-graph");
        for (index, level) in stage_levels(&jobs).into_iter().enumerate() {
            if index > 0 {
                graph = graph.child(div().class("cv-graph-arrow").text("↓"));
            }
            let mut row = div().class("cv-graph-level");
            for stage in level {
                let mut card = div()
                    .class("cv-stage")
                    .attr("data-stage", stage[0].stage.clone());
                for job in stage {
                    card = card.child(job_block(&env, &state, &job.id));
                }
                row = row.child(card);
            }
            graph = graph.child(row);
        }
        graph.into_view()
    })
}

fn find_job(state: &State, id: &str) -> Option<Job> {
    state.run.with(|r| {
        r.as_ref()
            .and_then(|v| v.jobs.iter().find(|j| j.id == id).cloned())
    })
}

/// A job as a native disclosure; its log is asked for when it is first opened.
fn job_block(env: &Env, state: &State, id: &str) -> Element {
    let head = {
        let (env, state, id) = (env.clone(), state.clone(), id.to_string());
        View::dynamic(move || {
            let Some(job) = find_job(&state, &id) else {
                return View::Empty;
            };
            let now = (env.now)();
            div()
                .class("cv-job-state")
                .child(pill(&env, job.status))
                .child(div().class("cv-job-name").text(job.qualified_name()))
                .child(
                    job.reused_from_run
                        .as_ref()
                        .map(|_| span().class("cv-muted").text(env.t.t("ui_job_reused"))),
                )
                .child(job.exit_code.filter(|code| *code != 0).map(|code| {
                    span()
                        .class("cv-mono cv-muted")
                        .text(format!("exit {code}"))
                }))
                .child(span().class("cv-muted").text(format::elapsed(
                    job.started_at,
                    job.finished_at,
                    now,
                )))
                .into_view()
        })
    };
    let open = {
        let (state, ask, id) = (state.clone(), Rc::clone(&env.ask), id.to_string());
        move || {
            // Only a job that ran has output, and a stream is opened once however often it is toggled.
            let ran = find_job(&state, &id)
                .is_some_and(|j| !matches!(j.status, Status::Skipped | Status::Queued));
            if ran && !state.opened.with_untracked(|o| o.contains(&id)) {
                state.opened.update(|o| {
                    o.insert(id.clone());
                });
                ask(Request::OpenLog(id.clone()));
            }
        }
    };
    let body = {
        let (env, state, id) = (env.clone(), state.clone(), id.to_string());
        View::dynamic(move || {
            let Some(job) = find_job(&state, &id) else {
                return View::Empty;
            };
            if matches!(job.status, Status::Skipped | Status::Queued) {
                return div()
                    .class("cv-job-reason")
                    .text(job.error.unwrap_or_else(|| env.t.t("ui_job_did_not_run")))
                    .into_view();
            }
            log_view(&env, &state, &id)
        })
    };
    element("details")
        .class("cv-job")
        .attr("data-job", id.to_string())
        .child(
            element("summary")
                .class("cv-job-head")
                .on_click(open)
                .child(head),
        )
        .child(div().class("cv-job-body").child(body))
}

/// The log, as far as it arrived: raw and copy beside it, one line per entry, stderr told apart.
fn log_view(env: &Env, state: &State, id: &str) -> View {
    let log_id = format!("log-{id}");
    let toolbar = div()
        .class("cv-log-toolbar")
        .child(
            a().attr("href", format!("{}/api/v1/jobs/{id}/raw", env.base))
                .attr("target", "_blank")
                .attr("rel", "noopener")
                .attr("id", format!("raw-{id}"))
                .attr("title", env.t.t("ui_log_raw_tooltip"))
                .class("cv-log-action")
                .text("↗"),
        )
        .child(
            button()
                .attr("type", "button")
                .attr("id", format!("copy-{id}"))
                // `forge-shell`'s script copies the text of the element it names.
                .attr("data-copy-target", log_id.clone())
                .attr("title", env.t.t("ui_log_copy_tooltip"))
                .class("cv-log-action")
                .text("⧉"),
        );
    let count = {
        let (logs, id) = (state.logs.clone(), id.to_string());
        move || logs.with(|m| m.get(&id).map_or(0, |l| l.lines.len()))
    };
    let lines = {
        let (logs, id, count) = (state.logs.clone(), id.to_string(), count.clone());
        View::keyed(
            move || (0..count()).collect::<Vec<usize>>(),
            |index| *index,
            move |index| {
                let (stderr, text) = logs.with(|m| {
                    m.get(&id)
                        .and_then(|l| l.lines.get(index).cloned())
                        .unwrap_or_default()
                });
                span()
                    .class(if stderr {
                        "cv-log-line cv-log-stderr"
                    } else {
                        "cv-log-line"
                    })
                    .text(text)
                    .into_view()
            },
        )
    };
    let waiting_note = {
        let (env, logs, id) = (env.clone(), state.logs.clone(), id.to_string());
        View::dynamic(move || {
            let empty = logs.with(|m| {
                m.get(&id)
                    .is_none_or(|l: &JobLog| l.lines.is_empty() && !l.done)
            });
            if empty {
                div()
                    .class("cv-log-empty")
                    .text(env.t.t("ui_log_loading"))
                    .into_view()
            } else {
                View::Empty
            }
        })
    };
    div()
        .child(toolbar)
        .child(
            div()
                .attr("id", log_id)
                .class("cv-log")
                .child(waiting_note)
                .child(lines),
        )
        .into_view()
}

fn artifacts(env: &Env, state: &State) -> View {
    let (env, run) = (env.clone(), state.run.clone());
    View::dynamic(move || {
        let list = run.with(|r| r.as_ref().map(|v| v.artifacts.clone()).unwrap_or_default());
        if list.is_empty() {
            return View::Empty;
        }
        let mut rows = div().class("cv-meta-list");
        for item in list {
            rows = rows.child(
                div()
                    .class("cv-artifact")
                    .child(
                        a().attr("href", item.href.clone())
                            .attr("data-artifact", item.artifact.id.clone())
                            .text(item.artifact.name.clone()),
                    )
                    .child(item.artifact.digest.as_deref().map(|d| {
                        span()
                            .class("cv-mono cv-muted")
                            .text(format::short_digest(d))
                    })),
            );
        }
        panel(div().text(env.t.t("ui_artifacts_title")))
            .attr("id", "run-artifacts")
            .child(rows)
            .into_view()
    })
}
