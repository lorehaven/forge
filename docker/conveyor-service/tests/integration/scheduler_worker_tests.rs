//! The scheduler, end to end.
//!
//! A real repository, a real checkout, real processes, a real database. These
//! are the tests that would notice if any seam between phases came apart, and
//! the only ones that exercise a run the way a webhook will.

use crate::support::{Origin, database, register_repo, skipped};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::{Run, Status, Trigger};
use conveyor_service::executors::NativeExecutor;
use conveyor_service::providers::Providers;
use conveyor_service::scheduler::queue::{self, NewRun};
use conveyor_service::scheduler::spawn_pool;
use quench_db::prelude::Db;
use std::sync::Arc;
use std::time::Duration;

const PIPELINE: &str = r#"
on = { push = ["master"] }

[[stage]]
name = "build"
[[stage.job]]
steps = ["echo building", "echo built > artifact.txt"]

[[stage]]
name  = "test"
needs = ["build"]
[[stage.job]]
steps = ["test -f artifact.txt"]

[[stage]]
name  = "deploy"
needs = ["test"]
when  = "branch == 'master'"
[[stage.job]]
steps = ["echo deploying"]
"#;

/// Starts the scheduler with its own work directory, and keeps the directory
/// alive for the caller.
fn start_scheduler(db: &Db) -> tempfile::TempDir {
    let work = tempfile::tempdir().expect("temp dir");
    let config = ConveyorConfig {
        work_dir: work.path().to_path_buf(),
        max_concurrent_runs: 1,
        default_job_timeout_secs: 60,
        checkout_timeout_secs: 60,
        ..ConveyorConfig::default()
    };
    spawn_pool(
        db.clone(),
        config,
        Arc::new(NativeExecutor::new()),
        Arc::new(Providers::from_env()),
    );
    work
}

async fn queue_run(db: &Db, repo_id: &str, git_ref: &str, sha: &str) -> String {
    queue::enqueue(
        db,
        &NewRun {
            repo_id: repo_id.to_string(),
            trigger: Trigger::Push,
            git_ref: git_ref.to_string(),
            sha: sha.to_string(),
            message: None,
            delivery_id: None,
        },
    )
    .await
    .expect("enqueue")
    .run()
    .id
    .clone()
}

/// Waits for the scheduler to finish a run.
async fn settle(db: &Db, run_id: &str) -> Run {
    for _ in 0..600 {
        let run = queue::read_run(db, run_id)
            .await
            .expect("read run")
            .expect("the run should exist");
        if run.status.is_terminal() {
            return run;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the scheduler never finished the run");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_queued_run_is_checked_out_planned_and_executed() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_queued_run_is_checked_out_planned_and_executed");
    };

    let origin = Origin::with_pipeline(PIPELINE);
    let repo = register_repo(&db, "e2e", &origin.url()).await;
    let _work = start_scheduler(&db);

    let run_id = queue_run(&db, &repo.id, "refs/heads/master", &origin.sha).await;
    let run = settle(&db, &run_id).await;

    assert_eq!(run.status, Status::Success, "error was {:?}", run.error);
    assert!(run.finished_at.is_some());
    assert!(
        run.claimed_by.is_none(),
        "a finished run releases its worker"
    );

    let jobs = queue::list_jobs(&db, &run_id).await.expect("jobs");
    assert_eq!(jobs.len(), 3);
    assert!(
        jobs.iter().all(|job| job.status == Status::Success),
        "{jobs:?}"
    );

    // Output was persisted when each job finished.
    let build = jobs.iter().find(|j| j.stage == "build").expect("build");
    let logs = queue::read_logs(&db, &build.id, -1).await.expect("logs");
    assert!(
        logs.iter().any(|chunk| chunk.line == "building"),
        "expected the step's output to have been stored: {logs:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_condition_that_excludes_a_stage_is_recorded_not_hidden() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_condition_that_excludes_a_stage_is_recorded_not_hidden");
    };

    // The pipeline's `on` covers only master, so trigger this by hand - a
    // manual run is allowed whatever the patterns say, which lets the `when`
    // on the deploy stage be the thing that excludes it.
    let origin = Origin::with_pipeline(
        &PIPELINE.replace(r#"on = { push = ["master"] }"#, r#"on = { push = ["*"] }"#),
    );
    let repo = register_repo(&db, "e2e", &origin.url()).await;
    let _work = start_scheduler(&db);

    let run_id = queue_run(&db, &repo.id, "refs/heads/topic", &origin.sha).await;
    let run = settle(&db, &run_id).await;

    assert_eq!(run.status, Status::Success);

    let jobs = queue::list_jobs(&db, &run_id).await.expect("jobs");
    let deploy = jobs.iter().find(|j| j.stage == "deploy").expect("deploy");
    assert_eq!(deploy.status, Status::Skipped);
    assert!(
        deploy.error.as_deref().is_some_and(|e| e.contains("when")),
        "the run should say why it did not deploy: {:?}",
        deploy.error
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_stage_fails_the_run_and_skips_what_needed_it() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_failing_stage_fails_the_run_and_skips_what_needed_it");
    };

    let origin = Origin::with_pipeline(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = ["exit 2"]

[[stage]]
name  = "deploy"
needs = ["build"]
[[stage.job]]
steps = ["echo should not happen"]
"#,
    );
    let repo = register_repo(&db, "e2e", &origin.url()).await;
    let _work = start_scheduler(&db);

    let run_id = queue_run(&db, &repo.id, "refs/heads/master", &origin.sha).await;
    let run = settle(&db, &run_id).await;

    assert_eq!(run.status, Status::Failed);

    let jobs = queue::list_jobs(&db, &run_id).await.expect("jobs");
    let build = jobs.iter().find(|j| j.stage == "build").expect("build");
    let deploy = jobs.iter().find(|j| j.stage == "deploy").expect("deploy");

    assert_eq!(build.status, Status::Failed);
    assert_eq!(build.exit_code, Some(2));

    // The plan could not know this: `when` is decided before anything runs, and
    // failure is only known afterwards.
    assert_eq!(deploy.status, Status::Skipped);
    assert!(
        deploy.error.as_deref().is_some_and(|e| e.contains("build")),
        "the skipped stage should name what failed: {:?}",
        deploy.error
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_commit_with_no_pipeline_fails_with_a_clear_reason() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_commit_with_no_pipeline_fails_with_a_clear_reason");
    };

    let origin = Origin::bare();
    let repo = register_repo(&db, "e2e", &origin.url()).await;
    let _work = start_scheduler(&db);

    let run_id = queue_run(&db, &repo.id, "refs/heads/master", &origin.sha).await;
    let run = settle(&db, &run_id).await;

    assert_eq!(run.status, Status::Failed);
    assert!(
        run.error
            .as_deref()
            .is_some_and(|e| e.contains(".conveyor.toml")),
        "{:?}",
        run.error
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unparseable_pipeline_fails_the_run_before_anything_executes() {
    let Some((db, _guard)) = database().await else {
        return skipped("an_unparseable_pipeline_fails_the_run_before_anything_executes");
    };

    let origin = Origin::with_pipeline(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = [{ npm = "install" }]
"#,
    );
    let repo = register_repo(&db, "e2e", &origin.url()).await;
    let _work = start_scheduler(&db);

    let run_id = queue_run(&db, &repo.id, "refs/heads/master", &origin.sha).await;
    let run = settle(&db, &run_id).await;

    assert_eq!(run.status, Status::Failed);
    assert!(
        run.error.as_deref().is_some_and(|e| e.contains("npm")),
        "{:?}",
        run.error
    );
    assert!(
        queue::list_jobs(&db, &run_id)
            .await
            .expect("jobs")
            .is_empty(),
        "nothing should have been planned"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ref_the_pipeline_does_not_want_is_skipped_not_failed() {
    // Registering a repository means conveyor watches it; the pipeline decides
    // which of those events it actually wants.
    let Some((db, _guard)) = database().await else {
        return skipped("a_ref_the_pipeline_does_not_want_is_skipped_not_failed");
    };

    let origin = Origin::with_pipeline(PIPELINE);
    let repo = register_repo(&db, "e2e", &origin.url()).await;
    let _work = start_scheduler(&db);

    let run_id = queue_run(&db, &repo.id, "refs/heads/scratch", &origin.sha).await;
    let run = settle(&db, &run_id).await;

    assert_eq!(run.status, Status::Skipped);
    assert!(
        run.error.as_deref().is_some_and(|e| e.contains("scratch")),
        "{:?}",
        run.error
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_running_run_stops_it() {
    let Some((db, _guard)) = database().await else {
        return skipped("cancelling_a_running_run_stops_it");
    };

    let origin = Origin::with_pipeline(
        r#"
[[stage]]
name = "build"
[[stage.job]]
steps = ["sleep 120"]
"#,
    );
    let repo = register_repo(&db, "e2e", &origin.url()).await;
    let _work = start_scheduler(&db);

    let run_id = queue_run(&db, &repo.id, "refs/heads/master", &origin.sha).await;

    // Wait until the job is actually going, so this cancels a running process
    // rather than a run that has not been claimed yet.
    for _ in 0..200 {
        let started = queue::list_jobs(&db, &run_id)
            .await
            .expect("jobs")
            .iter()
            .any(|job| job.status == Status::Running);
        if started {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    assert!(queue::request_cancel(&db, &run_id).await.expect("cancel"));

    let run = settle(&db, &run_id).await;
    assert_eq!(run.status, Status::Cancelled, "error was {:?}", run.error);
}
