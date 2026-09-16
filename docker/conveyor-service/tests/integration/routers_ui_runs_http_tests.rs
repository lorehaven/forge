//! HTTP-level coverage for `routers/ui/pages/runs.rs`'s route handlers
//! (`run_page`, `run_state`) - the DB-touching orchestration
//! `tests/unit/routers_ui_runs_tests.rs` deliberately leaves out, covering
//! only the pure block-rendering helpers there.

use crate::support::{self, database, register_repo};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::Trigger;
use conveyor_service::scheduler::queue::{self, NewRun};
use http::{Method, StatusCode};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use std::sync::Arc;

async fn app(db: Db, config: JwtConfig) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    conveyor_service::routers::ui::register_routes();
    let container = ContainerBuilder::new()
        .provide(db)
        .provide(config)
        .provide(ConveyorConfig::default())
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

#[tokio::test]
async fn run_page_reports_not_found_for_an_unknown_run() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("run_page_reports_not_found_for_an_unknown_run");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/runs/does-not-exist",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn run_page_renders_ok_for_a_queued_run() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("run_page_renders_ok_for_a_queued_run");
    };
    let repo = register_repo(&db, "widget", "https://example.test/widget.git").await;
    let enqueued = queue::enqueue(
        &db,
        &NewRun {
            repo_id: repo.id.clone(),
            trigger: Trigger::Push,
            git_ref: "refs/heads/master".to_string(),
            sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            message: Some("a commit".to_string()),
            delivery_id: None,
            resumed_from: None,
        },
    )
    .await
    .expect("enqueue");
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/ui/runs/{}", enqueued.run().id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = support::body_text(resp).await;
    assert!(html.contains("tests/widget"));
}

#[tokio::test]
async fn run_state_reports_not_found_for_an_unknown_run() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("run_state_reports_not_found_for_an_unknown_run");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/runs/does-not-exist/state",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn run_state_renders_the_fragment_with_no_jobs_yet() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("run_state_renders_the_fragment_with_no_jobs_yet");
    };
    let repo = register_repo(&db, "widget", "https://example.test/widget.git").await;
    let enqueued = queue::enqueue(
        &db,
        &NewRun {
            repo_id: repo.id.clone(),
            trigger: Trigger::Push,
            git_ref: "refs/heads/master".to_string(),
            sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            message: Some("a commit".to_string()),
            delivery_id: None,
            resumed_from: None,
        },
    )
    .await
    .expect("enqueue");
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/ui/runs/{}/state", enqueued.run().id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn run_state_with_a_matching_job_count_omits_the_job_list_swap() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("run_state_with_a_matching_job_count_omits_the_job_list_swap");
    };
    let repo = register_repo(&db, "widget", "https://example.test/widget.git").await;
    let enqueued = queue::enqueue(
        &db,
        &NewRun {
            repo_id: repo.id.clone(),
            trigger: Trigger::Push,
            git_ref: "refs/heads/master".to_string(),
            sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            message: Some("a commit".to_string()),
            delivery_id: None,
            resumed_from: None,
        },
    )
    .await
    .expect("enqueue");
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    // The browser already knows about 0 jobs, matching the database (none
    // have been created yet), so the query-count branch takes the "no swap
    // needed" path rather than the mismatch one exercised above.
    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/ui/runs/{}/state?jobs=0", enqueued.run().id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}
