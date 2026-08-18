//! One run: what it built, what each job did, and its output.
//!
//! Logs stream over server-sent events. A finished job's log arrives complete
//! and the stream ends; a running job's arrives as it happens. The page does
//! not need to know which, because the endpoint does not either.
//!
//! Everything else on the page - the pills, the durations, the artifacts - is
//! polled, because none of it is append-only the way a log is. A run's state
//! lives in the database rather than in the worker holding it, so any replica
//! can answer, which the log stream cannot claim.
//!
//! The poll deliberately does not re-render the job bodies. Replacing those
//! every two seconds would tear down whatever log stream is open inside them,
//! which is why the mutable parts of each job carry their own id and arrive as
//! out-of-band swaps around the `<details>` rather than through them.

use crate::domain::{Artifact, Job, Repo, Run, Status};
use crate::routers::ui::common::{
    UiPageKind, format, is_ui_authenticated, render_page, status_pill, ui_login_redirect,
    ui_login_redirect_for, ui_path,
};
use crate::scheduler::{queue, repos};
use actix_web::{HttpRequest, HttpResponse, Responder, get, http::header::ContentType, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use serde::Deserialize;

/// How often a moving run is asked about. Fast enough that a job changing state
/// feels immediate, slow enough that a page left open overnight on a finished
/// run costs nothing - it stops entirely once the run rests.
const POLL_INTERVAL: &str = "every 2s";

#[get("/runs/{id}")]
pub(super) async fn run_page(
    request: HttpRequest,
    path: web::Path<String>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    if !is_ui_authenticated(&request, &config).await {
        return ui_login_redirect();
    }

    let run = match queue::read_run(&db, &path).await {
        Ok(Some(run)) => run,
        Ok(None) => return not_found(),
        Err(error) => {
            tracing::error!("could not read run {}: {error}", path.as_str());
            return HttpResponse::ServiceUnavailable().body(error.to_string());
        }
    };

    let repo = repos::read(&db, &run.repo_id).await.ok().flatten();
    let jobs = queue::list_jobs(&db, &run.id).await.unwrap_or_default();
    let artifacts = queue::list_artifacts(&db, &run.id)
        .await
        .unwrap_or_default();

    render_page(
        HttpResponse::Ok(),
        content()
            .class("home-content")
            .child(page(&run, repo.as_ref(), &jobs, &artifacts)),
        UiPageKind::Home,
    )
}

/// What the page reports it already has, so the fragment can tell whether the
/// job list it is answering about is the one the browser is looking at.
#[derive(Deserialize)]
pub(super) struct StateQuery {
    jobs: Option<usize>,
}

/// The polled half of the run page.
///
/// Answers with the state block htmx swaps in place, plus out-of-band elements
/// for the parts that live outside it. The whole job list is sent only when the
/// browser's count disagrees with the database's - that is the run being
/// planned, where the page went from no jobs to all of them, and the only
/// moment at which replacing the list can cost nothing.
#[get("/runs/{id}/state")]
pub(super) async fn run_state(
    request: HttpRequest,
    path: web::Path<String>,
    query: web::Query<StateQuery>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    // The fragment-aware form: this is polled every two seconds, so it is the
    // request most likely to be the one that meets an expired session.
    if !is_ui_authenticated(&request, &config).await {
        return ui_login_redirect_for(&request);
    }

    let run = match queue::read_run(&db, &path).await {
        Ok(Some(run)) => run,
        Ok(None) => return HttpResponse::NotFound().finish(),
        Err(error) => {
            tracing::error!("could not read run {}: {error}", path.as_str());
            return HttpResponse::ServiceUnavailable().finish();
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

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(body)
}

/// Marks an element as replacing the one with its id, wherever that sits.
fn oob(element: Element) -> Element {
    element.attr("hx-swap-oob", "true")
}

fn not_found() -> HttpResponse {
    render_page(
        HttpResponse::NotFound(),
        content().class("home-content").child(
            div()
                .class("home-container")
                .child(empty_state("ui_run_not_found")),
        ),
        UiPageKind::Home,
    )
}

fn page(run: &Run, repo: Option<&Repo>, jobs: &[Job], artifacts: &[Artifact]) -> Element {
    div()
        .class("home-container")
        // The same three blocks the fragment answers with, rendered by the same
        // functions. Two renderers producing markup that has to agree is how
        // they stop agreeing.
        .child(state_block(run, repo, jobs.len()))
        .child(jobs_block(jobs))
        .child(artifacts_block(artifacts))
}

/// The run's own state, and the element that asks for it again.
///
/// A resting run carries no `hx-trigger`, so the swap that reports it finished
/// is also the one that stops the polling - there is no separate signal to send
/// and nothing to miss.
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

pub fn jobs_block(jobs: &[Job]) -> Element {
    let mut list = div().attr("id", "run-jobs");
    for job in jobs {
        list = list.child(job_block(job));
    }
    list
}

/// Always rendered, even with nothing in it: an out-of-band swap needs
/// something already on the page to replace, and a run gains its artifacts
/// while somebody is watching.
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

/// One job, as a native disclosure.
///
/// `<details>` gives collapse and expand with no script at all, and htmx loads
/// the body the first time it is opened - so a run with eight jobs opens eight
/// log streams only if somebody actually opens all eight.
pub fn job_block(job: &Job) -> Element {
    // Only a job that ran has output. A skipped one shows why instead, and
    // needs no request to say so.
    let ran = !matches!(job.status, Status::Skipped | Status::Queued);

    let mut summary = element("summary").class("job-head").child(job_state(job));

    let body = if ran {
        // `once`, so collapsing and reopening does not fetch it again - and
        // does not open a second stream.
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

/// The parts of a job's headline that change while it runs.
///
/// Its own element so the poll can replace it without touching the `<summary>`
/// around it: that summary carries the `click once` that fetches the log, and
/// re-rendering it would arm the trigger again - a second click would then open
/// a second stream over the first.
///
/// `.job-state` is `display: contents`, so wrapping these four children changes
/// what can be swapped and not how the row is laid out.
pub fn job_state(job: &Job) -> Element {
    div()
        .attr("id", format!("job-state-{}", job.id))
        .class("job-state")
        .child(status_pill(job.status))
        .child(div().class("job-name").text(job.qualified_name()))
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
