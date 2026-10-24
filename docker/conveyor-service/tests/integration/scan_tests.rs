//! `crate::scan::latest` against a real run's steps and logs.
//!
//! `record_steps`/`append_logs` write the same shape a real job leaves behind
//! (see `executors::native::Emitter` and `queue::record_steps`), so this
//! exercises the full read path - `list_runs` -> `list_jobs` -> `list_steps`
//! -> `read_logs` -> the per-check parsers - the way the scan page itself
//! does, not just the parsers in isolation.

use crate::support::{database, register_repo, skipped};
use conveyor_service::domain::{Status, Trigger};
use conveyor_service::executors::{LogChunk, StepState, Stream};
use conveyor_service::scan;
use conveyor_service::scheduler::queue::{self, NewRun};
use quench_core::time::{Duration, Time};

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

/// The steps of one job that ran `anvil` lint, machete and audit, one after another from `t0`.
fn anvil_steps(t0: quench_core::time::Time) -> Vec<StepState> {
    vec![
        StepState {
            ordinal: 0,
            kind: "anvil".to_string(),
            command: "lint --all-targets --deny-warnings --json".to_string(),
            status: Status::Failed,
            exit_code: Some(1),
            started_at: Some(t0),
            finished_at: Some(t0 + Duration::seconds(1)),
        },
        StepState {
            ordinal: 1,
            kind: "anvil".to_string(),
            command: "machete --json".to_string(),
            status: Status::Success,
            exit_code: Some(0),
            started_at: Some(t0 + Duration::seconds(2)),
            finished_at: Some(t0 + Duration::seconds(3)),
        },
        StepState {
            ordinal: 2,
            kind: "anvil".to_string(),
            command: "audit --json".to_string(),
            status: Status::Success,
            exit_code: Some(0),
            started_at: Some(t0 + Duration::seconds(4)),
            finished_at: Some(t0 + Duration::seconds(5)),
        },
    ]
}

// Each anvil `--json` step's stdout is its report; a real run would also
// carry stderr progress chunks interleaved by timestamp, which `scan`
// now filters out (see `collect_job_checks`) - included here as
// `Stream::Stderr` chunks to prove that filtering actually happens
// rather than merely being untested.
fn anvil_logs(t0: quench_core::time::Time) -> Vec<LogChunk> {
    vec![
        chunk(
            0,
            t0,
            Stream::Stdout,
            r#"{"reason":"compiler-message","message":{"level":"warning","message":"unused variable: `x`","spans":[{"file_name":"src/main.rs","line_start":10,"column_start":9,"is_primary":true}]}}"#,
        ),
        chunk(1, t0, Stream::Stderr, "   Compiling foo v0.1.0"),
        chunk(
            2,
            t0,
            Stream::Stdout,
            r#"{"reason":"compiler-message","message":{"level":"error","message":"could not compile `foo`"}}"#,
        ),
        chunk(
            3,
            t0 + Duration::seconds(2),
            Stream::Stdout,
            r#"{"findings":[]}"#,
        ),
        chunk(
            4,
            t0 + Duration::seconds(2),
            Stream::Stderr,
            "Analyzing dependencies of crates in this directory...",
        ),
        chunk(
            5,
            t0 + Duration::seconds(4),
            Stream::Stdout,
            r#"{"vulnerabilities":{"found":false,"count":0,"list":[]},"warnings":{}}"#,
        ),
    ]
}

#[quench_test::test]
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

    let t0 = Time::now();
    let steps = anvil_steps(t0);
    queue::record_steps(&db, &job.id, &steps)
        .await
        .expect("record steps");

    let logs = anvil_logs(t0);
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

#[quench_test::test]
async fn a_repo_with_no_runs_has_an_empty_summary() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_repo_with_no_runs_has_an_empty_summary");
    };
    let repo = register_repo(&db, "never-built", "file:///nowhere").await;

    let summary = scan::latest(&db, &repo.id).await.expect("scan summary");

    assert!(summary.run.is_none());
    assert!(summary.is_empty());
}

#[quench_test::test]
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

    let t0 = Time::now();
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

fn chunk(seq: u64, at: quench_core::time::Time, stream: Stream, line: &str) -> LogChunk {
    LogChunk {
        seq,
        stream,
        line: line.to_string(),
        at,
    }
}

#[quench_test::test]
async fn the_tests_summary_a_job_left_in_warehouse_is_read_back() {
    use conveyor_service::artifacts::WarehouseStore;
    use conveyor_service::domain::Artifact;
    use quench_test::mock::matchers::{method, path};
    use quench_test::mock::{Mock, MockServer, ResponseTemplate};

    let Some((db, _guard)) = database().await else {
        return skipped("the_tests_summary_a_job_left_in_warehouse_is_read_back");
    };
    let repo = register_repo(&db, "tested", "file:///nowhere").await;
    let enqueued = queue::enqueue(&db, &new_run(&repo.id, SHA))
        .await
        .expect("enqueue");
    let run_id = enqueued.run().id.clone();
    let jobs = queue::create_jobs(
        &db,
        &run_id,
        &[conveyor_service::scheduler::queue::PlannedJob {
            stage: "test".to_string(),
            name: "unit".to_string(),
            needs: vec![],
            status: Status::Success,
            error: None,
            reused_from_run: None,
        }],
    )
    .await
    .expect("create job");

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/files/artifacts/file"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"duration_ms": 1000, "summary": {"total": 2, "passed": 1, "failed": 1, "skipped": 0},
                "suites": [{"name": "s", "cases": [{"name": "s::bad", "status": "failed", "message": "boom"}]}]}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    let uri = format!(
        "{}/api/v1/files/artifacts/file?path=conveyor%2F{run_id}%2Fsummary.json",
        server.uri()
    );
    queue::record_artifact(
        &db,
        &Artifact {
            id: quench_core::uuid::Uuid::new_v4().to_string(),
            run_id,
            job_id: jobs[0].id.clone(),
            kind: "file".to_string(),
            name: scan::TESTS_SUMMARY.to_string(),
            version: None,
            uri,
            digest: None,
            created_at: Time::now(),
        },
    )
    .await
    .expect("record artifact");

    queue::record_artifact(
        &db,
        &Artifact {
            id: quench_core::uuid::Uuid::new_v4().to_string(),
            run_id: enqueued.run().id.clone(),
            job_id: jobs[0].id.clone(),
            kind: "file".to_string(),
            name: scan::TESTS_REPORT_PAGE.to_string(),
            version: None,
            uri: format!(
                "{}/api/v1/files/artifacts/file?path=index.html",
                server.uri()
            ),
            digest: None,
            created_at: Time::now(),
        },
    )
    .await
    .expect("record report page");

    let store = WarehouseStore::new(&server.uri(), "artifacts", None);
    let summary = scan::latest_with(&db, &repo.id, Some(&store))
        .await
        .expect("scan summary");
    let tests = summary.tests.expect("a tests result");
    assert!(
        tests
            .report_uri
            .as_deref()
            .is_some_and(|uri| uri.ends_with("/api/v1/files/artifacts/file?path=index.html"))
    );
    assert!(!tests.passed);
    assert_eq!(tests.job_name, "test/unit");
    assert_eq!(tests.findings[0].title, "s::bad");

    // Asked again, the artifact is not fetched a second time (the mock expects exactly one request).
    let again = scan::latest_with(&db, &repo.id, Some(&store))
        .await
        .expect("scan summary");
    assert!(again.tests.is_some());

    // No store, no test summary.
    let without = scan::latest_with(&db, &repo.id, None).await.expect("scan");
    assert!(without.tests.is_none());
}

/// The page that serves a stored report only has one to serve for a repository whose latest run kept one.
#[quench_test::test]
async fn the_report_page_is_not_found_without_a_stored_report() {
    use quench_auth::domain::jwt::JwtConfig;
    use quench_http::http::{Method, StatusCode};

    let Some((db, _guard)) = database().await else {
        return skipped("the_report_page_is_not_found_without_a_stored_report");
    };
    register_repo(&db, "reportless", "file:///nowhere").await;
    conveyor_service::routers::ui::register_routes();
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = false;
    let (app, container) = crate::support::app(db, config).await;

    for path in [
        "/ui/repos/tests/reportless/scan/tests/report",
        "/ui/repos/nobody/nothing/scan/tests/report",
    ] {
        let resp = app
            .call(crate::support::req(Method::GET, path, &container))
            .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{path}");
    }
}
