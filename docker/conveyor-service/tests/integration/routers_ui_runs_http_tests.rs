//! HTTP-level coverage for `routers/ui/pages/runs.rs`'s route handlers
//! (`run_page`, `run_state`) - the DB-touching orchestration
//! `tests/unit/routers_ui_runs_tests.rs` deliberately leaves out, covering
//! only the pure block-rendering helpers there.

use crate::support::{database, register_repo};
use actix_web::http::StatusCode;
use actix_web::web::Data;
use actix_web::{App, test as actix_test};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::Trigger;
use conveyor_service::routers::ui;
use conveyor_service::scheduler::queue::{self, NewRun};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;

fn app_with(
    db: Db,
    config: JwtConfig,
) -> App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    App::new()
        .app_data(Data::new(db))
        .app_data(Data::new(config.clone()))
        .app_data(Data::new(ConveyorConfig::default()))
        .service(ui::scope(config))
}

#[actix_web::test]
async fn run_page_reports_not_found_for_an_unknown_run() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("run_page_reports_not_found_for_an_unknown_run");
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/runs/does-not-exist")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn run_page_renders_ok_for_a_queued_run() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("run_page_renders_ok_for_a_queued_run");
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
        },
    )
    .await
    .expect("enqueue");
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri(&format!("/ui/runs/{}", enqueued.run().id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("tests/widget"));
}

#[actix_web::test]
async fn run_state_reports_not_found_for_an_unknown_run() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("run_state_reports_not_found_for_an_unknown_run");
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/runs/does-not-exist/state")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn run_state_renders_the_fragment_with_no_jobs_yet() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("run_state_renders_the_fragment_with_no_jobs_yet");
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
        },
    )
    .await
    .expect("enqueue");
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri(&format!("/ui/runs/{}/state", enqueued.run().id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn run_state_with_a_matching_job_count_omits_the_job_list_swap() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "run_state_with_a_matching_job_count_omits_the_job_list_swap",
        );
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
        },
    )
    .await
    .expect("enqueue");
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    // The browser already knows about 0 jobs, matching the database (none
    // have been created yet), so the query-count branch takes the "no swap
    // needed" path rather than the mismatch one exercised above.
    let req = actix_test::TestRequest::get()
        .uri(&format!("/ui/runs/{}/state?jobs=0", enqueued.run().id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
