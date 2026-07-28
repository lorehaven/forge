//! Unit tests for `executors/native.rs`.
//!
//! These spawn real processes. The executor's whole job is to run a command and
//! report what happened, so a fake `Command` would test the arrangement of the
//! code rather than the behaviour anyone depends on.

use conveyor_service::domain::Status;
use conveyor_service::executors::engine::{Handle, JobExecutor, JobSpec, JobState, Stream};
use conveyor_service::executors::native::NativeExecutor;
use conveyor_service::pipeline::Step;
use conveyor_service::secrets::Redactor;
use conveyor_service::workspace::Workspace;
use std::collections::BTreeMap;
use std::time::Duration;

fn spec(name: &str, steps: Vec<Step>) -> JobSpec {
    JobSpec {
        id: format!("job-{name}"),
        name: name.to_string(),
        steps,
        env: BTreeMap::new(),
        timeout: Duration::from_secs(30),
        image: None,
        source: None,
        redactor: Redactor::none(),
    }
}

fn run(command: &str) -> Step {
    Step::Run(command.to_string())
}

/// Polls until the job reaches a resting state, or gives up.
async fn finish(executor: &NativeExecutor, handle: &Handle) -> JobState {
    for _ in 0..600 {
        let state = executor.poll(handle).await.expect("poll");
        if state.is_finished() {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("job never finished");
}

async fn output(executor: &NativeExecutor, handle: &Handle) -> Vec<String> {
    executor
        .logs(handle)
        .await
        .expect("logs")
        .history
        .into_iter()
        .map(|chunk| chunk.line)
        .collect()
}

fn workspace() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().expect("temp dir");
    let workspace = Workspace::new(dir.path().to_path_buf());
    (dir, workspace)
}

// ---------------------------------------------------------------------------
// Success
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_passing_job_succeeds_and_records_every_step() {
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor
        .start(&spec("build", vec![run("true"), run("true")]), &ws)
        .await
        .expect("start");

    let state = finish(&executor, &handle).await;
    assert_eq!(state.status, Status::Success);
    assert_eq!(state.exit_code, Some(0));
    assert!(state.error.is_none());
    assert!(state.started_at.is_some() && state.finished_at.is_some());
    assert_eq!(state.steps.len(), 2);
    assert!(state.steps.iter().all(|s| s.status == Status::Success));
}

#[tokio::test]
async fn stdout_is_captured() {
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor
        .start(&spec("build", vec![run("echo hello from the step")]), &ws)
        .await
        .expect("start");
    finish(&executor, &handle).await;

    assert!(
        output(&executor, &handle)
            .await
            .iter()
            .any(|line| line == "hello from the step"),
        "expected the step's output in the log"
    );
}

#[tokio::test]
async fn the_command_is_echoed_before_it_runs() {
    // Without this a log opens on the output of a command nobody can identify.
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor
        .start(&spec("build", vec![run("echo hi")]), &ws)
        .await
        .expect("start");
    finish(&executor, &handle).await;

    let lines = output(&executor, &handle).await;
    assert_eq!(lines.first().map(String::as_str), Some("$ echo hi"));
}

#[tokio::test]
async fn stderr_is_captured_and_labelled() {
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor
        .start(&spec("build", vec![run("echo oops >&2")]), &ws)
        .await
        .expect("start");
    finish(&executor, &handle).await;

    let chunks = executor.logs(&handle).await.expect("logs").history;
    assert!(
        chunks
            .iter()
            .any(|c| c.stream == Stream::Stderr && c.line == "oops"),
        "expected 'oops' on stderr, got {chunks:?}"
    );
}

#[tokio::test]
async fn log_sequence_numbers_are_contiguous_and_ordered() {
    // A reader resumes by asking for everything after the seq it has; a gap
    // would silently drop output and a repeat would duplicate it.
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor
        .start(
            &spec("build", vec![run("echo one; echo two; echo three >&2")]),
            &ws,
        )
        .await
        .expect("start");
    finish(&executor, &handle).await;

    let chunks = executor.logs(&handle).await.expect("logs").history;
    for (index, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.seq, index as u64, "seq should be contiguous");
    }
}

#[tokio::test]
async fn steps_run_in_the_checkout() {
    let executor = NativeExecutor::new();
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("marker"), b"x").expect("write");
    let ws = Workspace::new(dir.path().to_path_buf());

    let handle = executor
        .start(&spec("build", vec![run("cat marker")]), &ws)
        .await
        .expect("start");

    assert_eq!(finish(&executor, &handle).await.status, Status::Success);
}

#[tokio::test]
async fn job_environment_reaches_the_step() {
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let mut job = spec("build", vec![run("echo \"[$GREETING]\"")]);
    job.env
        .insert("GREETING".to_string(), "from the pipeline".to_string());

    let handle = executor.start(&job, &ws).await.expect("start");
    finish(&executor, &handle).await;

    assert!(
        output(&executor, &handle)
            .await
            .iter()
            .any(|line| line == "[from the pipeline]"),
        "the job's env should reach the step"
    );
}

#[tokio::test]
async fn steps_run_in_order() {
    let executor = NativeExecutor::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let ws = Workspace::new(dir.path().to_path_buf());

    let handle = executor
        .start(
            &spec(
                "build",
                vec![run("echo first >> order"), run("echo second >> order")],
            ),
            &ws,
        )
        .await
        .expect("start");
    finish(&executor, &handle).await;

    assert_eq!(
        std::fs::read_to_string(dir.path().join("order")).expect("read"),
        "first\nsecond\n"
    );
}

// ---------------------------------------------------------------------------
// Failure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_failing_step_fails_the_job_with_its_exit_code() {
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor
        .start(&spec("build", vec![run("exit 3")]), &ws)
        .await
        .expect("start");

    let state = finish(&executor, &handle).await;
    assert_eq!(state.status, Status::Failed);
    assert_eq!(state.exit_code, Some(3));
    assert_eq!(state.steps[0].status, Status::Failed);
    assert_eq!(state.steps[0].exit_code, Some(3));
}

#[tokio::test]
async fn a_failing_step_skips_the_ones_after_it() {
    let executor = NativeExecutor::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let ws = Workspace::new(dir.path().to_path_buf());

    let handle = executor
        .start(
            &spec("build", vec![run("exit 1"), run("touch should-not-exist")]),
            &ws,
        )
        .await
        .expect("start");

    let state = finish(&executor, &handle).await;
    assert_eq!(state.steps[1].status, Status::Skipped);
    assert!(
        !dir.path().join("should-not-exist").exists(),
        "a step after a failure must not run"
    );
}

#[tokio::test]
async fn no_step_is_left_looking_like_it_is_still_queued() {
    // A finished job whose later steps still read `queued` looks like it is
    // still going.
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor
        .start(
            &spec("build", vec![run("exit 1"), run("true"), run("true")]),
            &ws,
        )
        .await
        .expect("start");

    let state = finish(&executor, &handle).await;
    assert!(state.steps.iter().all(|s| s.status != Status::Queued));
}

#[tokio::test]
async fn a_step_whose_command_does_not_exist_fails_the_job() {
    // Deliberately a `run` step. A tool step would depend on whether `anvil`
    // happens to be on PATH, which differs between a developer's machine and
    // the service image - the test would then be asserting the environment.
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor
        .start(
            &spec("build", vec![run("definitely-not-a-real-command-xyz")]),
            &ws,
        )
        .await
        .expect("start");

    let state = finish(&executor, &handle).await;
    assert_eq!(state.status, Status::Failed);
    // 127 is what a POSIX shell returns for "command not found".
    assert_eq!(state.exit_code, Some(127));
}

#[tokio::test]
async fn a_job_with_no_steps_is_refused() {
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();
    assert!(executor.start(&spec("build", vec![]), &ws).await.is_err());
}

#[tokio::test]
async fn a_quoting_mistake_is_caught_before_anything_runs() {
    // Step two cannot be resolved; step one must not have run by the time the
    // caller learns about it.
    let executor = NativeExecutor::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let ws = Workspace::new(dir.path().to_path_buf());

    let result = executor
        .start(
            &spec(
                "build",
                vec![
                    run("touch ran-anyway"),
                    Step::Anvil("release --message 'unclosed".to_string()),
                ],
            ),
            &ws,
        )
        .await;

    assert!(result.is_err(), "start should refuse the job");
    assert!(!dir.path().join("ran-anyway").exists());
}

// ---------------------------------------------------------------------------
// Cancellation and timeouts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cancelling_stops_a_running_job() {
    let executor = NativeExecutor::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let ws = Workspace::new(dir.path().to_path_buf());

    let handle = executor
        .start(
            &spec(
                "build",
                vec![run("sleep 60"), run("touch should-not-exist")],
            ),
            &ws,
        )
        .await
        .expect("start");

    // Let the child actually start, so this cancels a running process rather
    // than a job that has not begun.
    tokio::time::sleep(Duration::from_millis(300)).await;
    executor.cancel(&handle).await.expect("cancel");

    let state = finish(&executor, &handle).await;
    assert_eq!(state.status, Status::Cancelled);
    assert_eq!(state.steps[0].status, Status::Cancelled);
    assert_eq!(state.steps[1].status, Status::Skipped);
    assert!(!dir.path().join("should-not-exist").exists());
}

#[tokio::test]
async fn a_job_that_outlives_its_timeout_fails() {
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let mut job = spec("build", vec![run("sleep 60")]);
    job.timeout = Duration::from_millis(400);

    let handle = executor.start(&job, &ws).await.expect("start");
    let state = finish(&executor, &handle).await;

    // Failed rather than cancelled: nobody asked it to stop, and the code was
    // not shown to work.
    assert_eq!(state.status, Status::Failed);
    assert!(
        state
            .error
            .as_deref()
            .is_some_and(|e| e.contains("timeout")),
        "{:?}",
        state.error
    );
}

#[tokio::test]
async fn the_timeout_covers_the_job_not_each_step() {
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let mut job = spec(
        "build",
        vec![run("sleep 0.4"), run("sleep 0.4"), run("sleep 5")],
    );
    job.timeout = Duration::from_millis(900);

    let handle = executor.start(&job, &ws).await.expect("start");
    let state = finish(&executor, &handle).await;

    assert_eq!(state.status, Status::Failed);
    assert_eq!(state.steps[0].status, Status::Success);
    assert_eq!(state.steps[1].status, Status::Success);
    assert_eq!(state.steps[2].status, Status::Failed);
}

// ---------------------------------------------------------------------------
// Handles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_unknown_handle_is_an_error_on_every_call() {
    let executor = NativeExecutor::new();
    let handle = Handle::new("never-started");

    assert!(executor.poll(&handle).await.is_err());
    assert!(executor.logs(&handle).await.is_err());
    assert!(executor.cancel(&handle).await.is_err());
}

#[tokio::test]
async fn forgetting_a_job_releases_it() {
    // Without this a long-lived service accumulates every log line it has
    // ever produced.
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor
        .start(&spec("build", vec![run("true")]), &ws)
        .await
        .expect("start");
    finish(&executor, &handle).await;

    executor.forget(&handle).await.expect("forget");
    assert!(executor.poll(&handle).await.is_err());
}

#[tokio::test]
async fn a_late_subscriber_still_sees_earlier_output() {
    // History and the live channel together, because either alone is a race.
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor
        .start(&spec("build", vec![run("echo early")]), &ws)
        .await
        .expect("start");
    finish(&executor, &handle).await;

    let tail = executor.logs(&handle).await.expect("logs");
    assert!(tail.history.iter().any(|c| c.line == "early"));
}

#[tokio::test]
async fn a_subscriber_receives_output_as_it_happens() {
    let executor = NativeExecutor::new();
    let (_dir, ws) = workspace();

    let handle = executor
        .start(&spec("build", vec![run("sleep 0.3; echo live")]), &ws)
        .await
        .expect("start");

    let mut tail = executor.logs(&handle).await.expect("logs");

    let received = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match tail.live.recv().await {
                Ok(chunk) if chunk.line == "live" => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    })
    .await
    .expect("should not time out");

    assert!(received, "expected the line to arrive on the live channel");
}
