//! HTTP-level coverage for `routers/ui/pages/pipelines.rs`'s `runs_list_page`
//! handler - the DB-touching orchestration left out of the crate's own
//! `tests/unit/routers_ui_pages_pipelines_tests.rs`, which covers only the
//! pure pager/header helpers.

use crate::support::{database, register_repo};
use actix_web::http::StatusCode;
use actix_web::web::Data;
use actix_web::{App, test as actix_test};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::Trigger;
use conveyor_service::routers::ui;
use conveyor_service::scheduler::projects::{self, NewProject};
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
async fn runs_list_page_renders_ok_with_no_runs() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("runs_list_page_renders_ok_with_no_runs");
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get().uri("/ui/runs").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn runs_list_page_reports_not_found_for_an_unknown_project_scope() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "runs_list_page_reports_not_found_for_an_unknown_project_scope",
        );
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/runs?project=does-not-exist")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn runs_list_page_lists_a_queued_run_scoped_to_its_project_and_a_later_page() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "runs_list_page_lists_a_queued_run_scoped_to_its_project_and_a_later_page",
        );
    };
    let repo = register_repo(&db, "widget", "https://example.test/widget.git").await;
    queue::enqueue(
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
    let app = actix_test::init_service(app_with(db.clone(), JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri(&format!("/ui/runs?project={}&page=1", repo.project_id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("tests/widget"));
}

#[actix_web::test]
async fn runs_list_page_scoped_to_an_unrelated_project_shows_no_runs() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "runs_list_page_scoped_to_an_unrelated_project_shows_no_runs",
        );
    };
    let repo = register_repo(&db, "widget", "https://example.test/widget.git").await;
    queue::enqueue(
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
    let other_project = projects::create(
        &db,
        &NewProject {
            name: "unrelated".to_string(),
            parent_id: None,
        },
    )
    .await
    .expect("create the project");
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri(&format!("/ui/runs?project={}", other_project.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(!html.contains("tests/widget"));
}
