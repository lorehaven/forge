//! Unit tests for `executors/mock.rs`.
//!
//! The mock is test infrastructure, so it gets tested too: a scheduler test
//! that fails because the mock lied is the worst kind to debug.

use conveyor_service::domain::Status;
use conveyor_service::executors::engine::{Handle, JobExecutor, JobSpec};
use conveyor_service::executors::mock::{MockExecutor, MockOutcome};
use conveyor_service::pipeline::Step;
use conveyor_service::secrets::Redactor;
use conveyor_service::workspace::Workspace;
use std::collections::BTreeMap;
use std::time::Duration;

fn spec(name: &str) -> JobSpec {
    JobSpec {
        id: format!("job-{name}"),
        name: name.to_string(),
        steps: vec![Step::Run("whatever".to_string())],
        env: BTreeMap::new(),
        timeout: Duration::from_secs(30),
        image: None,
        source: None,
        redactor: Redactor::none(),
    }
}

fn workspace() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().expect("temp dir");
    let workspace = Workspace::new(dir.path().to_path_buf());
    (dir, workspace)
}

#[tokio::test]
async fn jobs_succeed_by_default() {
    let executor = MockExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor.start(&spec("build"), &ws).await.expect("start");
    let state = executor.poll(&handle).await.expect("poll");

    assert_eq!(state.status, Status::Success);
    assert!(
        state.is_finished(),
        "a mock job is done the moment it starts"
    );
}

#[tokio::test]
async fn an_outcome_can_be_scripted_by_job_name() {
    // By name rather than by id, so a test scripts "the deploy job fails"
    // without knowing the id the scheduler will generate.
    let executor = MockExecutor::new();
    let (_dir, ws) = workspace();
    executor.set_outcome("deploy", MockOutcome::failure(2));

    let passed = executor.start(&spec("build"), &ws).await.expect("start");
    let failed = executor.start(&spec("deploy"), &ws).await.expect("start");

    assert_eq!(
        executor.poll(&passed).await.expect("poll").status,
        Status::Success
    );

    let state = executor.poll(&failed).await.expect("poll");
    assert_eq!(state.status, Status::Failed);
    assert_eq!(state.exit_code, Some(2));
}

#[tokio::test]
async fn the_default_outcome_applies_to_unscripted_jobs() {
    let executor = MockExecutor::new();
    let (_dir, ws) = workspace();
    executor.set_default_outcome(MockOutcome::failure(1));

    let handle = executor.start(&spec("anything"), &ws).await.expect("start");
    assert_eq!(
        executor.poll(&handle).await.expect("poll").status,
        Status::Failed
    );
}

#[tokio::test]
async fn scripted_lines_come_back_as_logs() {
    let executor = MockExecutor::new();
    let (_dir, ws) = workspace();
    executor.set_outcome(
        "build",
        MockOutcome::success().with_lines(["compiling", "done"]),
    );

    let handle = executor.start(&spec("build"), &ws).await.expect("start");
    let lines: Vec<String> = executor
        .logs(&handle)
        .await
        .expect("logs")
        .history
        .into_iter()
        .map(|chunk| chunk.line)
        .collect();

    assert_eq!(lines, ["compiling", "done"]);
}

#[tokio::test]
async fn every_step_reports_the_jobs_outcome() {
    let executor = MockExecutor::new();
    let (_dir, ws) = workspace();
    executor.set_default_outcome(MockOutcome::failure(7));

    let mut job = spec("build");
    job.steps = vec![
        Step::Run("one".to_string()),
        Step::Anvil("build --all".to_string()),
    ];

    let handle = executor.start(&job, &ws).await.expect("start");
    let state = executor.poll(&handle).await.expect("poll");

    assert_eq!(state.steps.len(), 2);
    assert_eq!(state.steps[1].kind, "anvil");
    assert_eq!(state.steps[1].command, "build --all");
    assert!(state.steps.iter().all(|s| s.status == Status::Failed));
}

#[tokio::test]
async fn started_jobs_are_recorded_in_order() {
    // This is how a test asserts that a stage was skipped: no job from it ever
    // started, rather than re-inspecting the plan.
    let executor = MockExecutor::new();
    let (_dir, ws) = workspace();

    executor.start(&spec("build"), &ws).await.expect("start");
    executor.start(&spec("test"), &ws).await.expect("start");

    assert_eq!(executor.started_names(), ["build", "test"]);
}

#[tokio::test]
async fn cancellations_are_recorded() {
    let executor = MockExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor.start(&spec("build"), &ws).await.expect("start");
    executor.cancel(&handle).await.expect("cancel");

    assert_eq!(executor.cancelled(), [handle]);
}

#[tokio::test]
async fn a_late_cancel_does_not_rewrite_a_finished_status() {
    // Which is what a real executor does with a cancel that arrives after the
    // job is over.
    let executor = MockExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor.start(&spec("build"), &ws).await.expect("start");
    executor.cancel(&handle).await.expect("cancel");

    assert_eq!(
        executor.poll(&handle).await.expect("poll").status,
        Status::Success
    );
}

#[tokio::test]
async fn a_job_with_no_steps_is_refused() {
    let executor = MockExecutor::new();
    let (_dir, ws) = workspace();

    let mut job = spec("build");
    job.steps.clear();
    assert!(executor.start(&job, &ws).await.is_err());
}

#[tokio::test]
async fn an_unknown_handle_is_an_error() {
    let executor = MockExecutor::new();
    let handle = Handle::new("never-started");

    assert!(executor.poll(&handle).await.is_err());
    assert!(executor.logs(&handle).await.is_err());
    assert!(executor.cancel(&handle).await.is_err());
}

#[tokio::test]
async fn forgetting_a_job_releases_it() {
    let executor = MockExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor.start(&spec("build"), &ws).await.expect("start");
    executor.forget(&handle).await.expect("forget");

    assert!(executor.poll(&handle).await.is_err());
}
