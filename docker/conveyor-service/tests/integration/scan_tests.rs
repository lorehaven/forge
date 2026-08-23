//! `crate::scan::latest` against a real run's steps and logs.
//!
//! `record_steps`/`append_logs` write the same shape a real job leaves behind
//! (see `executors::native::Emitter` and `queue::record_steps`), so this
//! exercises the full read path - `list_runs` -> `list_jobs` -> `list_steps`
//! -> `read_logs` -> the per-check parsers - the way the scan page itself
//! does, not just the parsers in isolation.

use crate::support::{database, register_repo, skipped};
use chrono::{Duration, Utc};
use conveyor_service::domain::{Status, Trigger};
use conveyor_service::executors::{LogChunk, StepState, Stream};
use conveyor_service::scan;
use conveyor_service::scheduler::queue::{self, NewRun};

fn new_run(repo_id: &str, sha: &str) -> NewRun {
    NewRun {
        repo_id: repo_id.to_string(),
        trigger: Trigger::Push,
        git_ref: "refs/heads/master".to_string(),
        sha: sha.to_string(),
        message: Some("a commit".to_string()),
        delivery_id: None,
        resumed_from: None,
    }
}

const SHA: &str = "cccccccccccccccccccccccccccccccccccccccc";

#[tokio::test]
async fn summarises_lint_machete_and_audit_from_one_job() {
    let Some((db, _guard)) = database().await else {
        return skipped("summarises_lint_machete_and_audit_from_one_job");
    };
    let repo = register_repo(&db, "scanned", "file:///nowhere").await;

    let enqueued = queue::enqueue(&db, &new_run(&repo.id, SHA))
        .await
        .expect("enqueue");
    let jobs = queue::create_jobs(
        &db,
        &enqueued.run().id,
        &[conveyor_service::scheduler::queue::PlannedJob {
            stage: "quality".to_string(),
            name: "checks".to_string(),
            needs: vec![],
            status: Status::Success,
            error: None,
            reused_from_run: None,
        }],
    )
    .await
    .expect("create job");
    let job = &jobs[0];

    let t0 = Utc::now();
    let steps = vec![
        StepState {
            ordinal: 0,
            kind: "anvil".to_string(),
            command: "lint --all-targets --deny-warnings".to_string(),
            status: Status::Failed,
            exit_code: Some(1),
            started_at: Some(t0),
            finished_at: Some(t0 + Duration::seconds(1)),
        },
        StepState {
            ordinal: 1,
            kind: "anvil".to_string(),
            command: "machete".to_string(),
            status: Status::Success,
            exit_code: Some(0),
            started_at: Some(t0 + Duration::seconds(2)),
            finished_at: Some(t0 + Duration::seconds(3)),
        },
        StepState {
            ordinal: 2,
            kind: "anvil".to_string(),
            command: "audit".to_string(),
            status: Status::Success,
            exit_code: Some(0),
            started_at: Some(t0 + Duration::seconds(4)),
            finished_at: Some(t0 + Duration::seconds(5)),
        },
    ];
    queue::record_steps(&db, &job.id, &steps)
        .await
        .expect("record steps");

    let logs = vec![
        chunk(0, t0, "warning: unused variable: `x`"),
        chunk(1, t0, "  --> src/main.rs:10:9"),
        chunk(2, t0, "error: could not compile `foo`"),
        chunk(
            3,
            t0 + Duration::seconds(2),
            "cargo-machete didn't find any unused dependencies in this directory. Good job!",
        ),
        chunk(4, t0 + Duration::seconds(4), "Scanning Cargo.lock"),
        chunk(5, t0 + Duration::seconds(4), "0 vulnerabilities found"),
    ];
    queue::append_logs(&db, &job.id, &logs)
        .await
        .expect("append logs");

    let summary = scan::latest(&db, &repo.id).await.expect("scan summary");

    assert!(!summary.is_empty());
    let lint = summary.lint.expect("lint result");
    assert!(!lint.passed);
    assert_eq!(lint.headline, "1 warning, 1 error");
    assert_eq!(lint.findings.len(), 2);
    assert_eq!(lint.findings[0].title, "unused variable: `x`");
    assert_eq!(lint.findings[0].severity.as_deref(), Some("warning"));
    assert_eq!(
        lint.findings[0].location.as_deref(),
        Some("src/main.rs:10:9")
    );

    let machete = summary.machete.expect("machete result");
    assert!(machete.passed);
    assert_eq!(machete.headline, "clean");
    assert!(machete.findings.is_empty());

    let audit = summary.audit.expect("audit result");
    assert!(audit.passed);
    assert_eq!(audit.headline, "clean");
    assert!(audit.findings.is_empty());
}

#[tokio::test]
async fn a_repo_with_no_runs_has_an_empty_summary() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_repo_with_no_runs_has_an_empty_summary");
    };
    let repo = register_repo(&db, "never-built", "file:///nowhere").await;

    let summary = scan::latest(&db, &repo.id).await.expect("scan summary");

    assert!(summary.run.is_none());
    assert!(summary.is_empty());
}

#[tokio::test]
async fn a_run_with_no_quality_steps_has_an_empty_summary() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_run_with_no_quality_steps_has_an_empty_summary");
    };
    let repo = register_repo(&db, "build-only", "file:///nowhere").await;

    let enqueued = queue::enqueue(&db, &new_run(&repo.id, SHA))
        .await
        .expect("enqueue");
    let jobs = queue::create_jobs(
        &db,
        &enqueued.run().id,
        &[conveyor_service::scheduler::queue::PlannedJob {
            stage: "build".to_string(),
            name: "build".to_string(),
            needs: vec![],
            status: Status::Success,
            error: None,
            reused_from_run: None,
        }],
    )
    .await
    .expect("create job");

    let t0 = Utc::now();
    queue::record_steps(
        &db,
        &jobs[0].id,
        &[StepState {
            ordinal: 0,
            kind: "anvil".to_string(),
            command: "build".to_string(),
            status: Status::Success,
            exit_code: Some(0),
            started_at: Some(t0),
            finished_at: Some(t0 + Duration::seconds(1)),
        }],
    )
    .await
    .expect("record steps");

    let summary = scan::latest(&db, &repo.id).await.expect("scan summary");

    assert!(summary.run.is_some());
    assert!(summary.is_empty());
}

fn chunk(seq: u64, at: chrono::DateTime<Utc>, line: &str) -> LogChunk {
    LogChunk {
        seq,
        stream: Stream::Stdout,
        line: line.to_string(),
        at,
    }
}
