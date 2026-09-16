//! HTTP-level coverage for `routers/ui/pages/home.rs`'s route handlers
//! (`home`, `home_slash`, `project_page`, `home_state`, `run_now`) - the
//! DB-touching orchestration `tests/unit/routers_ui_home_tests.rs`
//! deliberately leaves out, covering only the pure chip/panel render
//! helpers there.

use crate::support::{self, database, register_repo};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::scheduler::projects::{self, NewProject};
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
async fn home_renders_ok_with_nothing_registered() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("home_renders_ok_with_nothing_registered");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(Method::GET, "/ui/home", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn home_slash_renders_ok() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("home_slash_renders_ok");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(Method::GET, "/ui/home/", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn home_lists_a_registered_repository() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("home_lists_a_registered_repository");
    };
    register_repo(&db, "widget", "https://example.test/widget.git").await;
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(Method::GET, "/ui/home", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = support::body_text(resp).await;
    assert!(html.contains("tests/widget"));
}

#[tokio::test]
async fn project_page_renders_ok_for_a_known_project() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("project_page_renders_ok_for_a_known_project");
    };
    let project = projects::create(
        &db,
        &NewProject {
            name: "root".to_string(),
            parent_id: None,
        },
    )
    .await
    .expect("create the project");
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/ui/projects/{}", project.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn project_page_reports_not_found_for_an_unknown_project() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("project_page_reports_not_found_for_an_unknown_project");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/projects/does-not-exist",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn home_state_renders_the_runs_fragment() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("home_state_renders_the_runs_fragment");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(Method::GET, "/ui/home/state", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn home_state_scoped_to_a_project_renders_ok() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("home_state_scoped_to_a_project_renders_ok");
    };
    let project = projects::create(
        &db,
        &NewProject {
            name: "root".to_string(),
            parent_id: None,
        },
    )
    .await
    .expect("create the project");
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/ui/home/state?project={}", project.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn run_now_for_an_unknown_repo_still_renders_the_runs_fragment() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("run_now_for_an_unknown_repo_still_renders_the_runs_fragment");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::POST,
            "/ui/home/repos/does-not-exist/run",
            &container,
        ))
        .await;
    // `trigger_manual`'s error is logged and swallowed, not surfaced as a
    // failed response - the fragment still renders.
    assert_eq!(resp.status(), StatusCode::OK);
}
