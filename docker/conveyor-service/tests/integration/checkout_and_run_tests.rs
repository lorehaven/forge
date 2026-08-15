//! Everything built so far, in the order the scheduler will use it.
//!
//! Checkout, then parse the `.conveyor.toml` that came with the commit, then
//! plan against the run's context, then execute what the plan says to execute.
//! Each of those is unit-tested on its own; this asserts that the seams between
//! them line up, which is the part unit tests cannot see.
//!
//! The scheduler does this for real now (`scheduler_worker_tests`). This stays
//! because it needs no database: it is the one test that can tell a broken
//! checkout or a broken plan from a broken queue.

use crate::support::Origin;
use conveyor_service::domain::Status;
use conveyor_service::executors::engine::{Handle, JobExecutor, JobSpec, JobState};
use conveyor_service::executors::native::NativeExecutor;
use conveyor_service::pipeline::{self, EvalContext, PIPELINE_FILE, Step};
use conveyor_service::secrets::Redactor;
use conveyor_service::workspace::{CheckoutRequest, Workspace, checkout};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

const PIPELINE: &str = r#"
on = { push = ["master", "release/*"] }

[[stage]]
name = "build"
[[stage.job]]
steps = ["echo building > built.txt", { run = "echo compiled" }]

[[stage]]
name  = "test"
needs = ["build"]
[[stage.job]]
steps = ["test -f built.txt"]

[[stage]]
name  = "deploy"
needs = ["test"]
when  = "branch == 'master'"
[[stage.job]]
steps = ["echo deploying > deployed.txt"]
"#;

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

/// What a run did: which jobs ran, and how each ended.
struct Outcome {
    ran: Vec<String>,
    skipped: Vec<String>,
    statuses: Vec<(String, Status)>,
}

async fn wait(executor: &NativeExecutor, handle: &Handle) -> JobState {
    for _ in 0..600 {
        let state = executor.poll(handle).await.expect("poll");
        if state.is_finished() {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("job never finished");
}

/// Checks the commit out, reads its pipeline, and runs what the plan says to.
async fn perform_run(origin: &Origin, git_ref: &str, work: &Path) -> (Workspace, Outcome) {
    let url = origin.url();
    let workspace = checkout(
        work,
        "run-1",
        &CheckoutRequest {
            clone_url: &url,
            git_ref,
            sha: &origin.sha,
            timeout: Duration::from_secs(60),
            credential: None,
        },
    )
    .await
    .expect("checkout");

    // Read from the checkout, not from the constant above: the point of an
    // in-repo pipeline is that the commit supplies it.
    let source = std::fs::read_to_string(workspace.root().join(PIPELINE_FILE))
        .expect("the commit should carry a pipeline");
    let spec = pipeline::parse(&source).expect("the pipeline should parse");

    let context = EvalContext::new("push", git_ref, &origin.sha);
    assert!(
        spec.on.allows("push", git_ref),
        "{git_ref} should trigger this pipeline"
    );

    let executor = NativeExecutor::new();
    let mut outcome = Outcome {
        ran: Vec::new(),
        skipped: Vec::new(),
        statuses: Vec::new(),
    };

    for stage_plan in pipeline::plan(&spec, &context) {
        let stage = &spec.stages[stage_plan.index];

        for job_plan in &stage_plan.jobs {
            let job = &stage.jobs[job_plan.index];
            let name = format!("{}/{}", stage.name, job.name);

            if !job_plan.decision.will_run() {
                outcome.skipped.push(name);
                continue;
            }

            let handle = executor
                .start(
                    &JobSpec {
                        id: format!("job-{name}"),
                        name: name.clone(),
                        steps: job.steps.clone(),
                        env: BTreeMap::new(),
                        timeout: Duration::from_secs(60),
                        image: job.image.clone(),
                        source: None,
                        redactor: Redactor::none(),
                    },
                    &workspace,
                )
                .await
                .expect("start");

            let state = wait(&executor, &handle).await;
            outcome.ran.push(name.clone());
            outcome.statuses.push((name, state.status));

            // A failed stage ends the run, which is what phase 4 will do too.
            if state.status.is_failure() {
                break;
            }
        }
    }

    (workspace, outcome)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_commit_on_master_runs_every_stage_in_order() {
    let origin = Origin::with_pipeline(PIPELINE);
    let work = tempfile::tempdir().expect("temp dir");

    let (workspace, outcome) = perform_run(&origin, "refs/heads/master", work.path()).await;

    assert_eq!(outcome.ran, ["build/build", "test/test", "deploy/deploy"]);
    assert!(outcome.skipped.is_empty());
    assert!(
        outcome
            .statuses
            .iter()
            .all(|(_, status)| *status == Status::Success),
        "{:?}",
        outcome.statuses
    );

    // The steps really ran, in the checkout, in order: `test` passing at all
    // means `build` wrote its file first.
    assert!(workspace.root().join("built.txt").exists());
    assert!(workspace.root().join("deployed.txt").exists());
}

#[tokio::test]
async fn a_commit_on_a_topic_branch_skips_the_deploy() {
    let origin = Origin::with_pipeline(PIPELINE);
    let work = tempfile::tempdir().expect("temp dir");

    // The pipeline's `on` covers `release/*`, so this triggers - and the
    // deploy stage's `when` is what excludes it, not the trigger.
    let (workspace, outcome) = perform_run(&origin, "refs/heads/release/1.2", work.path()).await;

    assert_eq!(outcome.ran, ["build/build", "test/test"]);
    assert_eq!(outcome.skipped, ["deploy/deploy"]);
    assert!(
        !workspace.root().join("deployed.txt").exists(),
        "a skipped stage must not have run"
    );
}

#[tokio::test]
async fn a_ref_the_pipeline_does_not_want_is_not_triggered() {
    let origin = Origin::with_pipeline(PIPELINE);
    let work = tempfile::tempdir().expect("temp dir");
    let url = origin.url();

    let workspace = checkout(
        work.path(),
        "run-1",
        &CheckoutRequest {
            clone_url: &url,
            git_ref: "refs/heads/master",
            sha: &origin.sha,
            timeout: Duration::from_secs(60),
            credential: None,
        },
    )
    .await
    .expect("checkout");

    let source = std::fs::read_to_string(workspace.root().join(PIPELINE_FILE)).expect("read");
    let spec = pipeline::parse(&source).expect("parses");

    assert!(!spec.on.allows("push", "refs/heads/scratch"));
    assert!(!spec.on.allows("pull_request", "refs/heads/master"));
    assert!(!spec.on.allows("push", "refs/tags/v1.0.0"));
}

#[tokio::test]
async fn a_failing_stage_stops_the_run() {
    let origin = Origin::with_pipeline(PIPELINE);
    let work = tempfile::tempdir().expect("temp dir");
    let url = origin.url();

    let workspace = checkout(
        work.path(),
        "run-1",
        &CheckoutRequest {
            clone_url: &url,
            git_ref: "refs/heads/master",
            sha: &origin.sha,
            timeout: Duration::from_secs(60),
            credential: None,
        },
    )
    .await
    .expect("checkout");

    let executor = NativeExecutor::new();
    let handle = executor
        .start(
            &JobSpec {
                id: "job-1".to_string(),
                name: "test/unit".to_string(),
                steps: vec![
                    Step::Run("exit 4".to_string()),
                    Step::Run("touch must-not-exist".to_string()),
                ],
                env: BTreeMap::new(),
                timeout: Duration::from_secs(60),
                image: None,
                source: None,
                redactor: Redactor::none(),
            },
            &workspace,
        )
        .await
        .expect("start");

    let state = wait(&executor, &handle).await;
    assert_eq!(state.status, Status::Failed);
    assert_eq!(state.exit_code, Some(4));
    assert!(!workspace.root().join("must-not-exist").exists());
}

#[tokio::test]
async fn the_workspace_is_removable_once_the_run_is_over() {
    let origin = Origin::with_pipeline(PIPELINE);
    let work = tempfile::tempdir().expect("temp dir");

    let (workspace, _) = perform_run(&origin, "refs/heads/master", work.path()).await;
    let root = workspace.root().to_path_buf();

    workspace.remove().await.expect("remove");
    assert!(!root.exists(), "a finished run leaves nothing behind");
}
