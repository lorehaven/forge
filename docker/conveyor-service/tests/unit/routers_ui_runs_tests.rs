//! Unit tests for `routers/ui/pages/runs.rs`.
//!
//! The run page is half static and half live, and the seam between the two is
//! the part worth pinning down: which elements the poll may replace, and which
//! it must leave alone because a log stream is attached to them.

use chrono::{Duration, Utc};
use conveyor_service::domain::{Artifact, Job, Run, Status, Trigger};
use conveyor_service::routers::ui::pages::runs::{
    artifacts_block, job_block, job_state, jobs_block, state_block,
};

fn run(status: Status) -> Run {
    Run {
        id: "run-1".to_string(),
        repo_id: "repo-1".to_string(),
        trigger: Trigger::Manual,
        git_ref: "refs/heads/master".to_string(),
        sha: "0123456789abcdef".to_string(),
        message: None,
        delivery_id: None,
        status,
        queued_at: Utc::now() - Duration::minutes(2),
        started_at: Some(Utc::now() - Duration::minutes(1)),
        finished_at: status.is_terminal().then(Utc::now),
        claimed_by: None,
        claimed_at: None,
        attempt: 1,
        error: None,
    }
}

fn job(status: Status) -> Job {
    Job {
        id: "job-1".to_string(),
        run_id: "run-1".to_string(),
        stage: "build".to_string(),
        name: "cargo".to_string(),
        needs: Vec::new(),
        status,
        exit_code: None,
        started_at: Some(Utc::now() - Duration::seconds(30)),
        finished_at: None,
        error: None,
    }
}

#[test]
fn a_moving_run_asks_again() {
    let html = state_block(&run(Status::Running), None, 3).render();

    assert!(html.contains(r#"hx-trigger="every 2s""#));
    assert!(html.contains(r#"hx-swap="outerHTML""#));
    // The count goes out with the request so the answer can tell whether the
    // browser is looking at the same job list the database has.
    assert!(html.contains("/runs/run-1/state?jobs=3"));
}

#[test]
fn a_queued_run_asks_again_too() {
    // The gap this closes: a run sits queued while it is checked out, and its
    // jobs do not exist yet. Stopping here would leave the page empty forever.
    let html = state_block(&run(Status::Queued), None, 0).render();

    assert!(html.contains(r#"hx-trigger="every 2s""#));
    assert!(html.contains("?jobs=0"));
}

#[test]
fn a_resting_run_stops_asking() {
    for status in [
        Status::Success,
        Status::Failed,
        Status::Cancelled,
        Status::Skipped,
    ] {
        let html = state_block(&run(status), None, 2).render();
        assert!(
            !html.contains("hx-trigger"),
            "{status} run kept polling; the swap that reports a run finished is \
             the one that has to stop the polling"
        );
    }
}

#[test]
fn the_polled_part_of_a_job_is_separable_from_its_log() {
    let block = job_block(&job(Status::Running)).render();

    // The summary owns the log fetch, and fires it at most once. The prefix
    // comes from BASE_PATH, which a unit test does not set, so only the tail is
    // asserted here.
    assert!(block.contains("jobs/job-1/log"));
    assert!(block.contains(r#"hx-trigger="click once""#));

    // What the poll replaces carries no behaviour of its own...
    let state = job_state(&job(Status::Running)).render();
    assert!(state.contains(r#"id="job-state-job-1""#));
    assert!(!state.contains("hx-get"));
    assert!(!state.contains("job-body"));

    // ...and sits inside the summary, ahead of the body that holds the stream.
    // Asserted by position rather than by substring because attribute order is
    // not stable across renders, and nothing here depends on it being.
    let summary_ends = block.find("</summary>").expect("a summary");
    let state_at = block.find("job-state-job-1").expect("the polled part");
    // `class="job-body"`, not `job-body`: the summary names it too, in the
    // `hx-target` that points at it.
    let body_at = block.find(r#"class="job-body""#).expect("the log body");

    assert!(
        state_at < summary_ends,
        "the polled part must be inside the summary: {block}"
    );
    assert!(
        body_at > summary_ends,
        "the log body must be outside it, or a poll would replace the stream: {block}"
    );
}

#[test]
fn a_running_job_reports_how_long_it_has_been_going() {
    let html = job_state(&job(Status::Running)).render();
    assert!(html.contains("so far"), "got: {html}");
}

fn artifact(name: &str) -> Artifact {
    Artifact {
        id: "artifact-1".to_string(),
        run_id: "run-1".to_string(),
        job_id: "job-1".to_string(),
        kind: "crate".to_string(),
        name: name.to_string(),
        version: Some("1.2.3".to_string()),
        uri: "https://warehouse.example/crates/thing/1.2.3".to_string(),
        digest: None,
        created_at: Utc::now(),
    }
}

#[test]
fn jobs_block_carries_the_swap_target_id_and_every_job() {
    let html = jobs_block(&[job(Status::Success), job(Status::Failed)]).render();
    assert!(html.contains(r#"id="run-jobs""#));
    assert_eq!(html.matches("class=\"job\"").count(), 2);
}

#[test]
fn jobs_block_is_still_the_swap_target_with_no_jobs_at_all() {
    let html = jobs_block(&[]).render();
    assert!(html.contains(r#"id="run-jobs""#));
}

#[test]
fn artifacts_block_is_present_but_empty_with_no_artifacts_yet() {
    // Always rendered (per the doc comment on `artifacts_block`) so an
    // out-of-band swap later has something to replace.
    let html = artifacts_block(&[]).render();
    assert!(html.contains(r#"id="run-artifacts""#));
}

#[test]
fn artifacts_block_lists_every_artifact_once_they_exist() {
    let html = artifacts_block(&[artifact("thing"), artifact("other-thing")]).render();
    assert!(html.contains(r#"id="run-artifacts""#));
    assert!(html.contains("thing"));
    assert!(html.contains("other-thing"));
}
