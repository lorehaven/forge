//! One run: what it built, what each job did, and its output. Logs stream
//! over SSE; everything else (pills, durations, artifacts) is polled instead.

use crate::domain::{Artifact, Job, Repo, Run, Status};
use crate::routers::ui::common::{
    PageAuth, PageGate, format, render_page, status_pill, ui_login_redirect, ui_path,
};
use crate::scheduler::{queue, repos};
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode, post};
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// Polling stops entirely once the run rests, so an overnight-open page costs nothing.
const POLL_INTERVAL: &str = "every 2s";

#[get("/ui/runs/{id}")]
pub(super) async fn run_page(
    PageAuth(authenticated): PageAuth,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }

    let run = match queue::read_run(&db, &id).await {
        Ok(Some(run)) => run,
        Ok(None) => return not_found(),
        Err(error) => {
            tracing::error!("could not read run {}: {error}", id);
            return Response::text(StatusCode::SERVICE_UNAVAILABLE, error.to_string());
        }
    };

    let repo = repos::read(&db, &run.repo_id).await.ok().flatten();
    let jobs = queue::list_jobs(&db, &run.id).await.unwrap_or_default();
    let artifacts = queue::list_artifacts(&db, &run.id)
        .await
        .unwrap_or_default();

    render_page(
        StatusCode::OK,
        content()
            .class("home-content")
            .child(page(&run, repo.as_ref(), &jobs, &artifacts)),
    )
}

/// What the page already has, so the fragment can tell if its job list is stale.
#[derive(Deserialize)]
pub(super) struct StateQuery {
    jobs: Option<usize>,
}

/// The polled half of the run page. The whole job list is sent only when the
/// browser's count disagrees with the database's (the run being planned).
#[get("/ui/runs/{id}/state")]
pub(super) async fn run_state(
    gate: PageGate,
    Path(id): Path<String>,
    Query(query): Query<StateQuery>,
    Inject(db): Inject<Db>,
) -> Response {
    // The fragment-aware form - polled every 2s, likeliest to meet an expired session.
    if let Err(response) = gate.or_redirect() {
        return response;
    }

    let run = match queue::read_run(&db, &id).await {
        Ok(Some(run)) => run,
        Ok(None) => return Response::new(StatusCode::NOT_FOUND),
        Err(error) => {
            tracing::error!("could not read run {}: {error}", id);
            return Response::new(StatusCode::SERVICE_UNAVAILABLE);
        }
    };

    let repo = repos::read(&db, &run.repo_id).await.ok().flatten();
    let jobs = queue::list_jobs(&db, &run.id).await.unwrap_or_default();
    let artifacts = queue::list_artifacts(&db, &run.id)
        .await
        .unwrap_or_default();

    let mut body = state_block(&run, repo.as_ref(), jobs.len()).render();

    for job in &jobs {
        body.push_str(&oob(job_state(job)).render());
    }
    body.push_str(&oob(artifacts_block(&artifacts)).render());

    if query.jobs != Some(jobs.len()) {
        body.push_str(&oob(jobs_block(&jobs)).render());
    }

    Response::html(StatusCode::OK, body)
}

/// `HX-Redirect`, not a swap - the result is a different run entirely, with nothing here to update in place.
#[post("/ui/runs/{id}/restart")]
pub(super) async fn run_restart(
    gate: PageGate,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    if let Err(response) = gate.or_redirect() {
        return response;
    }

    let destination = match crate::routers::api::runs::restart_run(&db, &id).await {
        Ok(run) => run.id,
        Err(error) => {
            tracing::warn!("restart of run {} could not start: {error}", id);
            id.clone()
        }
    };

    Response::new(StatusCode::OK).header("HX-Redirect", ui_path(&format!("/runs/{destination}")))
}

/// Marks an element as replacing the one with its id, wherever that sits.
fn oob(element: Element) -> Element {
    element.attr("hx-swap-oob", "true")
}

fn not_found() -> Response {
    render_page(
        StatusCode::NOT_FOUND,
        content().class("home-content").child(
            div()
                .class("home-container")
                .child(empty_state("ui_run_not_found")),
        ),
    )
}

fn page(run: &Run, repo: Option<&Repo>, jobs: &[Job], artifacts: &[Artifact]) -> Element {
    div()
        .class("home-container")
        // Same three blocks and functions the fragment answers with, to stay in sync.
        .child(state_block(run, repo, jobs.len()))
        .child(jobs_block(jobs))
        .child(artifacts_block(artifacts))
}

/// A resting run carries no `hx-trigger` - the swap that reports it finished also stops the polling.
pub fn state_block(run: &Run, repo: Option<&Repo>, job_count: usize) -> Element {
    let mut block = div()
        .attr("id", "run-state")
        .child(header_row(run, repo))
        .child(meta_row(run));

    if let Some(error) = &run.error {
        block = block.child(
            div()
                .class("panel")
                .child(
                    div()
                        .class("panel-title")
                        .attr("data-i18n", "ui_run_reason"),
                )
                .child(div().class("job-reason").text(error)),
        );
    }

    // A restart doesn't rebuild passed stages - see `worker::execute_jobs`.
    if run.status.is_failure() {
        block = block.child(
            button()
                .attr("type", "button")
                .class("run-button")
                .attr("data-i18n", "ui_run_restart")
                .attr("hx-post", ui_path(&format!("/runs/{}/restart", run.id)))
                .attr("hx-swap", "none"),
        );
    }

    if !run.status.is_terminal() {
        block = block
            .attr(
                "hx-get",
                ui_path(&format!("/runs/{}/state?jobs={job_count}", run.id)),
            )
            .attr("hx-trigger", POLL_INTERVAL)
            .attr("hx-swap", "outerHTML");
    }

    block
}

/// One row per dependency level, laid out the way `worker::execute_jobs` runs
/// them; `job_block` keeps its id so `job-state-{id}` OOB swaps still find it.
pub fn jobs_block(jobs: &[Job]) -> Element {
    let mut graph = div().attr("id", "run-jobs").class("job-graph");

    for (level_index, level) in stage_levels(jobs).into_iter().enumerate() {
        if level_index > 0 {
            graph = graph.child(
                div()
                    .class("job-graph-connector")
                    .child(i().class("fas").class("fa-arrow-down")),
            );
        }

        let mut row = div().class("job-graph-level");
        for stage_jobs in level {
            let mut card = div().class("stage-card");
            for job in stage_jobs {
                card = card.child(job_block(job));
            }
            row = row.child(card);
        }
        graph = graph.child(row);
    }

    graph
}

/// Level *n* needs only levels below it - the set of stages run concurrently.
/// Grouped from `Job` rows, not the pipeline spec, so old runs render the same.
fn stage_levels(jobs: &[Job]) -> Vec<Vec<Vec<&Job>>> {
    let mut stage_order: Vec<&str> = Vec::new();
    let mut jobs_by_stage: HashMap<&str, Vec<&Job>> = HashMap::new();
    let mut needs_by_stage: HashMap<&str, &[String]> = HashMap::new();

    for job in jobs {
        if !jobs_by_stage.contains_key(job.stage.as_str()) {
            stage_order.push(job.stage.as_str());
        }
        jobs_by_stage
            .entry(job.stage.as_str())
            .or_default()
            .push(job);
        needs_by_stage
            .entry(job.stage.as_str())
            .or_insert(job.needs.as_slice());
    }

    let mut level_of: HashMap<&str, usize> = HashMap::new();
    let mut visiting: HashSet<&str> = HashSet::new();
    for &stage in &stage_order {
        stage_level(stage, &needs_by_stage, &mut level_of, &mut visiting);
    }

    let level_count = level_of.values().copied().max().map_or(0, |max| max + 1);
    let mut levels: Vec<Vec<Vec<&Job>>> = vec![Vec::new(); level_count.max(1)];
    for &stage in &stage_order {
        levels[level_of[stage]].push(jobs_by_stage.remove(stage).unwrap_or_default());
    }
    levels.retain(|level| !level.is_empty());
    levels
}

/// `visiting` guards a `needs` cycle - the parser already refuses one, so
/// this is only a backstop for a hand-edited or migrated database.
fn stage_level<'a>(
    stage: &'a str,
    needs_by_stage: &HashMap<&'a str, &'a [String]>,
    level_of: &mut HashMap<&'a str, usize>,
    visiting: &mut HashSet<&'a str>,
) -> usize {
    if let Some(&level) = level_of.get(stage) {
        return level;
    }
    if !visiting.insert(stage) {
        return 0;
    }

    let needs = needs_by_stage.get(stage).copied().unwrap_or(&[]);
    let level = needs
        .iter()
        .filter(|need| needs_by_stage.contains_key(need.as_str()))
        .map(|need| stage_level(need, needs_by_stage, level_of, visiting) + 1)
        .max()
        .unwrap_or(0);

    visiting.remove(stage);
    level_of.insert(stage, level);
    level
}

/// Always rendered, even empty - an OOB swap needs something on the page to replace.
pub fn artifacts_block(artifacts: &[Artifact]) -> Element {
    let mut block = div().attr("id", "run-artifacts");
    if !artifacts.is_empty() {
        block = block.child(artifacts_panel(artifacts));
    }
    block
}

fn header_row(run: &Run, repo: Option<&Repo>) -> Element {
    div()
        .class("run-header")
        .child(status_pill(run.status))
        .child(h3().text(repo.map_or_else(|| "unknown repository".to_string(), Repo::slug)))
        .child(span().class("mono muted").text(run.ref_name()))
        .child(span().class("mono muted").text(run.short_sha()))
        .child_opt(
            run.message
                .as_ref()
                .map(|message| span().class("muted").text(message)),
        )
}

fn meta_row(run: &Run) -> Element {
    div()
        .class("run-meta")
        .child(labelled("ui_meta_trigger", &run.trigger.to_string()))
        .child(labelled("ui_meta_queued", &format::relative(run.queued_at)))
        .child(labelled(
            "ui_meta_duration",
            &format::elapsed(run.started_at, run.finished_at),
        ))
        .child(labelled("ui_meta_attempt", &run.attempt.to_string()))
}

fn labelled(key: &str, value: &str) -> Element {
    span()
        .child(span().class("muted").attr("data-i18n", key))
        .child(span().text(format!(" {value}")))
}

/// One job as a native `<details>` disclosure - htmx loads the log body only
/// when opened, so eight jobs open eight streams only if all eight are opened.
pub fn job_block(job: &Job) -> Element {
    // Only a job that ran has output; a skipped one shows why instead.
    let ran = !matches!(job.status, Status::Skipped | Status::Queued);

    let mut summary = element("summary").class("job-head").child(job_state(job));

    let body = if ran {
        // `once` - collapsing and reopening doesn't refetch or open a second stream.
        summary = summary
            .attr("hx-get", ui_path(&format!("/jobs/{}/log", job.id)))
            .attr("hx-target", "next .job-body")
            .attr("hx-swap", "innerHTML")
            .attr("hx-trigger", "click once");

        div()
            .class("job-body")
            .child(div().class("log-empty").attr("data-i18n", "ui_log_loading"))
    } else {
        div().class("job-body").child(
            div().class("job-reason").text(
                job.error
                    .clone()
                    .unwrap_or_else(|| "this job did not run".to_string()),
            ),
        )
    };

    element("details").class("job").child(summary).child(body)
}

/// Own element so the poll can replace it without re-rendering the `<summary>`
/// around it, which would re-arm its `click once` log-fetch trigger.
pub fn job_state(job: &Job) -> Element {
    div()
        .attr("id", format!("job-state-{}", job.id))
        .class("job-state")
        .child(status_pill(job.status))
        .child(div().class("job-name").text(job.qualified_name()))
        .child_opt(
            job.reused_from_run
                .as_ref()
                .map(|_| span().class("muted").attr("data-i18n", "ui_job_reused")),
        )
        .child_opt(
            job.exit_code
                .filter(|code| *code != 0)
                .map(|code| span().class("mono muted").text(format!("exit {code}"))),
        )
        .child(
            span()
                .class("muted")
                .text(format::elapsed(job.started_at, job.finished_at)),
        )
}

fn artifacts_panel(artifacts: &[Artifact]) -> Element {
    let mut panel = div().class("panel").child(
        div()
            .class("panel-title")
            .attr("data-i18n", "ui_artifacts_title"),
    );

    let mut list = div().class("meta-list");
    for artifact in artifacts {
        list = list.child(
            div()
                .class("artifact")
                .child(a().attr("href", &artifact.uri).text(&artifact.name))
                .child_opt(
                    artifact
                        .digest
                        .as_ref()
                        .map(|digest| span().class("mono muted").text(short_digest(digest))),
                ),
        );
    }

    panel = panel.child(list);
    panel
}

/// `sha256:0123abc…` - enough to compare by eye, short enough to sit in a row.
fn short_digest(digest: &str) -> String {
    match digest.split_once(':') {
        Some((algorithm, hex)) => format!("{algorithm}:{}…", &hex[..hex.len().min(12)]),
        None => digest.to_string(),
    }
}

pub(super) fn register_routes() {
    let _ = run_page as fn(_, _, _) -> _;
    let _ = run_state as fn(_, _, _, _) -> _;
    let _ = run_restart as fn(_, _, _) -> _;
}
