//! HTTP-level coverage for `routers/ui/pages/pipelines.rs`'s `runs_list_page`
//! handler - the DB-touching orchestration left out of the crate's own
//! `tests/unit/routers_ui_pages_pipelines_tests.rs`, which covers only the
//! pure pager/header helpers.

use crate::support::{self, database, register_repo};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::Trigger;
use conveyor_service::scheduler::projects::{self, NewProject};
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
async fn runs_list_page_renders_ok_with_no_runs() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("runs_list_page_renders_ok_with_no_runs");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(Method::GET, "/ui/runs", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn runs_list_page_reports_not_found_for_an_unknown_project_scope() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("runs_list_page_reports_not_found_for_an_unknown_project_scope");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/runs?project=does-not-exist",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn runs_list_page_lists_a_queued_run_scoped_to_its_project_and_a_later_page() {
    let Some((db, _guard)) = database().await else {
        return support::skipped(
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
    let (app, container) = app(db.clone(), JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/ui/runs?project={}&page=1", repo.project_id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = support::body_text(resp).await;
    assert!(html.contains("tests/widget"));
}

#[tokio::test]
async fn runs_list_page_scoped_to_an_unrelated_project_shows_no_runs() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("runs_list_page_scoped_to_an_unrelated_project_shows_no_runs");
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
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/ui/runs?project={}", other_project.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = support::body_text(resp).await;
    assert!(!html.contains("tests/widget"));
}
