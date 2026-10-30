//! "Run with parameters": a manual run limited to chosen stages, through the API, the form and a real
//! worker over a real repository. Needs `CONVEYOR_TEST_DATABASE_URL` - see `support.rs`.

use crate::support::{self, Origin, database, register_repo, skipped};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::{Run, Status, Trigger};
use conveyor_service::executors::NativeExecutor;
use conveyor_service::providers::Providers;
use conveyor_service::routers::api::runs::{restart_run, trigger_run};
use conveyor_service::scheduler::queue;
use conveyor_service::scheduler::spawn_pool;
use conveyor_service::scheduler::stages;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::http::{Method, StatusCode};
use std::sync::Arc;

const PIPELINE: &str = r#"
on = { push = ["master"], schedule = ["0 0 * * *"] }

[[stage]]
name = "check"
when = "event != 'schedule'"
[[stage.job]]
name = "lint"
steps = ["echo linting"]

[[stage]]
name  = "build"
needs = ["check"]
when  = "event != 'schedule'"
[[stage.job]]
name = "compile"
steps = ["echo building"]

[[stage]]
name = "security"
when = "event == 'schedule'"
[[stage.job]]
name = "nightly"
steps = ["echo scanning"]
"#;

fn start_scheduler(db: &Db) -> quench_core::tmp::TempDir {
    let work = quench_core::tmp::TempDir::new().expect("temp dir");
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

async fn settle(db: &Db, run_id: &str) -> Run {
    for _ in 0..600 {
        let run = queue::read_run(db, run_id).await.unwrap().unwrap();
        if run.status.is_terminal() {
            return run;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the scheduler never finished the run");
}

#[allow(clippy::unnecessary_wraps)] // compared against `Run::stages` and passed as `trigger_run`'s `Option`
fn names(values: &[&str]) -> Option<Vec<String>> {
    Some(values.iter().map(ToString::to_string).collect())
}

async fn started(db: &Db, repo_id: &str, stages: Option<Vec<String>>) -> Run {
    trigger_run(db, repo_id, None, None, Trigger::Manual, None, stages)
        .await
        .expect("the run starts")
        .run()
        .clone()
}

async fn job_status(db: &Db, run_id: &str, job: &str) -> Option<Status> {
    queue::list_jobs(db, run_id)
        .await
        .unwrap()
        .iter()
        .find(|j| j.name == job)
        .map(|j| j.status)
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stage_that_is_scheduled_only_runs_alone_when_it_is_asked_for() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_stage_that_is_scheduled_only_runs_alone_when_it_is_asked_for");
    };
    let origin = Origin::with_pipeline(PIPELINE);
    let repo = register_repo(&db, "limited", &origin.url()).await;
    let _work = start_scheduler(&db);

    // An ordinary manual run does what a push does: check and build, not the nightly stage.
    let plain = settle(&db, &started(&db, &repo.id, None).await.id).await;
    assert_eq!(plain.status, Status::Success, "{:?}", plain.error);
    assert_eq!(plain.stages, None);
    assert_eq!(
        job_status(&db, &plain.id, "lint").await,
        Some(Status::Success)
    );
    assert_ne!(
        job_status(&db, &plain.id, "nightly").await,
        Some(Status::Success)
    );

    // Asked for by name, the security stage runs - on its own: nothing else is planned at all.
    let only = settle(&db, &started(&db, &repo.id, names(&["security"])).await.id).await;
    assert_eq!(only.status, Status::Success, "{:?}", only.error);
    assert_eq!(only.trigger, Trigger::Manual);
    assert_eq!(only.stages, names(&["security"]));
    let jobs = queue::list_jobs(&db, &only.id).await.unwrap();
    assert_eq!(jobs.len(), 1, "{jobs:?}");
    assert_eq!(jobs[0].name, "nightly");
    assert_eq!(jobs[0].status, Status::Success);
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn stages_can_be_run_together_and_a_dependency_not_chosen_does_not_block() {
    let Some((db, _guard)) = database().await else {
        return skipped("stages_can_be_run_together_and_a_dependency_not_chosen_does_not_block");
    };
    let origin = Origin::with_pipeline(PIPELINE);
    let repo = register_repo(&db, "together", &origin.url()).await;
    let _work = start_scheduler(&db);

    // security with build - and build's `needs = ["check"]` is not chosen, so it is simply not waited for.
    let run = settle(
        &db,
        &started(&db, &repo.id, names(&["build", "security"]))
            .await
            .id,
    )
    .await;
    assert_eq!(run.status, Status::Success, "{:?}", run.error);
    assert_eq!(
        job_status(&db, &run.id, "compile").await,
        Some(Status::Success)
    );
    assert_eq!(
        job_status(&db, &run.id, "nightly").await,
        Some(Status::Success)
    );
    assert_eq!(
        job_status(&db, &run.id, "lint").await,
        None,
        "check was not chosen"
    );
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stage_that_does_not_exist_fails_the_run_and_says_which_there_are() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_stage_that_does_not_exist_fails_the_run_and_says_which_there_are");
    };
    let origin = Origin::with_pipeline(PIPELINE);
    let repo = register_repo(&db, "typo", &origin.url()).await;
    let _work = start_scheduler(&db);

    let run = settle(&db, &started(&db, &repo.id, names(&["securty"])).await.id).await;
    assert_eq!(run.status, Status::Failed);
    let error = run.error.expect("a reason");
    assert!(error.contains("securty"), "{error}");
    assert!(error.contains("check, build, security"), "{error}");
    assert!(queue::list_jobs(&db, &run.id).await.unwrap().is_empty());
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restart_keeps_the_selection() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_restart_keeps_the_selection");
    };
    // Only a failed run can be restarted, so the chosen stage fails.
    let origin = Origin::with_pipeline(
        r#"
[[stage]]
name = "build"
[[stage.job]]
name = "compile"
steps = ["echo building"]

[[stage]]
name = "flaky"
when = "event == 'schedule'"
[[stage.job]]
name = "check"
steps = ["exit 1"]
"#,
    );
    let repo = register_repo(&db, "restarted", &origin.url()).await;
    let _work = start_scheduler(&db);

    let first = settle(&db, &started(&db, &repo.id, names(&["flaky"])).await.id).await;
    assert_eq!(first.status, Status::Failed);
    assert_eq!(first.stages, names(&["flaky"]));

    let again = restart_run(&db, &first.id).await.expect("restart");
    assert_eq!(
        again.stages,
        names(&["flaky"]),
        "the restart is limited to the same stages"
    );
    let finished = settle(&db, &again.id).await;
    assert_eq!(finished.status, Status::Failed);
    assert_eq!(
        job_status(&db, &finished.id, "compile").await,
        None,
        "build was not selected"
    );
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_known_stages_come_from_recent_runs_and_start_out_the_way_an_ordinary_run_goes() {
    let Some((db, _guard)) = database().await else {
        return skipped(
            "the_known_stages_come_from_recent_runs_and_start_out_the_way_an_ordinary_run_goes",
        );
    };
    let origin = Origin::with_pipeline(PIPELINE);
    let repo = register_repo(&db, "known", &origin.url()).await;

    // Never run: nothing to offer yet.
    assert!(stages::known(&db, &repo.id).await.unwrap().is_empty());

    let _work = start_scheduler(&db);
    settle(&db, &started(&db, &repo.id, None).await.id).await;
    // A run limited to one stage planned only that stage; it must not shrink the list.
    settle(&db, &started(&db, &repo.id, names(&["security"])).await.id).await;

    let known = stages::known(&db, &repo.id).await.unwrap();
    let summary: Vec<(&str, bool)> = known
        .iter()
        .map(|s| (s.name.as_str(), s.on_by_default))
        .collect();
    // Dependencies first, ties by name; the nightly stage is offered but not ticked.
    assert_eq!(
        summary,
        [("check", true), ("build", true), ("security", false)]
    );
}

// --- the JSON API and the form ---

async fn app(
    db: Db,
) -> (
    Arc<dyn quench_http::endpoint::Endpoint>,
    Arc<quench_http::di::Container>,
) {
    conveyor_service::routers::api::register_routes();
    conveyor_service::routers::ui::register_routes();
    support::app(db, JwtConfig::for_tests()).await
}

#[quench_test::test]
async fn the_api_takes_stages_and_refuses_an_empty_selection() {
    let Some((db, _guard)) = database().await else {
        return skipped("the_api_takes_stages_and_refuses_an_empty_selection");
    };
    let origin = Origin::with_pipeline(PIPELINE);
    let repo = register_repo(&db, "api", &origin.url()).await;
    let (app, container) = app(db).await;
    let url = format!("/api/v1/repos/{}/runs", repo.id);

    let resp = app
        .call(support::json_req(
            Method::POST,
            &url,
            &quench_serde::json!({ "stages": ["security", " build ", "security"] }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let body = support::json_body(resp).await;
    assert_eq!(
        body["stages"],
        quench_serde::json!(["security", "build"]),
        "trimmed, no repeats"
    );

    for bad in [
        quench_serde::json!({ "stages": [] }),
        quench_serde::json!({ "stages": ["  ", ""] }),
    ] {
        let resp = app
            .call(support::json_req(Method::POST, &url, &bad, &container))
            .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{bad:?}");
    }
    // No `stages` at all is the whole pipeline, as ever.
    let resp = app
        .call(support::json_req(
            Method::POST,
            &url,
            &quench_serde::json!({}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    assert!(support::json_body(resp).await["stages"].is_null());

    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/api/v1/repos/{}/stages", repo.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}
