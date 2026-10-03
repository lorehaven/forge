use gantry_service::domain::cluster::JobStatus;
use gantry_service::domain::executor::{Executor, Launch, LocalExecutor};
use gantry_service::domain::settings::Settings;
use gantry_service::domain::steps::{Plan, Step};
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

fn runner_script(dir: &std::path::Path, body: &str) -> String {
    let path = dir.join("gantry-runner");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

fn launch(name: &str) -> Launch {
    Launch {
        operation_id: "op".into(),
        name: name.into(),
        plan: Plan {
            steps: vec![Step::Pull {
                package: "forge".into(),
                version: "1.0.0".into(),
            }],
            summary: vec![],
            ..Default::default()
        },
    }
}

fn executor(dir: &std::path::Path, body: &str, dry_run: bool) -> LocalExecutor {
    let mut settings = Settings::inert();
    settings.state_dir = dir.join("state");
    settings.runner_bin = runner_script(dir, body);
    LocalExecutor::new(settings, dry_run)
}

async fn until_done(executor: &LocalExecutor, name: &str) -> JobStatus {
    for _ in 0..100 {
        let status = executor.status(name).await.unwrap();
        if status != JobStatus::Active {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the runner never finished");
}

#[tokio::test]
async fn a_local_runner_is_a_child_process_whose_output_and_exit_code_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let executor = executor(
        dir.path(),
        r#"echo "plan=$GANTRY_PLAN"; echo "dry=$GANTRY_DRY_RUN"; echo "GANTRY-RESULT: ok""#,
        true,
    );

    assert_eq!(
        executor.status("gantry-op-a").await.unwrap(),
        JobStatus::Absent
    );
    executor.start(&launch("gantry-op-a")).await.unwrap();
    assert_eq!(
        until_done(&executor, "gantry-op-a").await,
        JobStatus::Succeeded
    );

    let log = executor.log("gantry-op-a").await.unwrap();
    assert!(log.contains(r#""step":"pull""#), "{log}");
    assert!(log.contains("dry=1"), "{log}");
}

#[tokio::test]
async fn a_failing_runner_is_a_failed_status() {
    let dir = tempfile::tempdir().unwrap();
    let executor = executor(dir.path(), "echo nope; exit 3", false);
    executor.start(&launch("gantry-op-b")).await.unwrap();
    assert_eq!(
        until_done(&executor, "gantry-op-b").await,
        JobStatus::Failed("the runner exited with 3".into())
    );
}

#[tokio::test]
async fn starting_twice_does_not_run_it_twice() {
    let dir = tempfile::tempdir().unwrap();
    let executor = executor(
        dir.path(),
        r#"echo run >> "$(dirname "$0")/runs"; echo "GANTRY-RESULT: ok""#,
        false,
    );
    executor.start(&launch("gantry-op-c")).await.unwrap();
    until_done(&executor, "gantry-op-c").await;
    executor.start(&launch("gantry-op-c")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let runs = std::fs::read_to_string(dir.path().join("runs")).unwrap();
    assert_eq!(runs.lines().count(), 1);
}

#[tokio::test]
async fn stopping_kills_the_runner_and_what_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let executor = executor(dir.path(), "sleep 30", false);
    executor.start(&launch("gantry-op-d")).await.unwrap();
    for _ in 0..40 {
        if executor.status("gantry-op-d").await.unwrap() == JobStatus::Active {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    executor.stop("gantry-op-d").await.unwrap();
    let status = until_done(&executor, "gantry-op-d").await;
    assert!(matches!(status, JobStatus::Failed(_)), "{status:?}");
}
