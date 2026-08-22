//! HTTP-level coverage for `routers/ui/pages/home.rs`'s route handlers
//! (`home`, `home_slash`, `project_page`, `home_state`, `run_now`) - the
//! DB-touching orchestration `tests/unit/routers_ui_home_tests.rs`
//! deliberately leaves out, covering only the pure chip/panel render
//! helpers there.

use crate::support::{database, register_repo};
use actix_web::http::StatusCode;
use actix_web::web::Data;
use actix_web::{App, test as actix_test};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::routers::ui;
use conveyor_service::scheduler::projects::{self, NewProject};
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
async fn home_renders_ok_with_nothing_registered() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("home_renders_ok_with_nothing_registered");
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get().uri("/ui/home").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn home_slash_renders_ok() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("home_slash_renders_ok");
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get().uri("/ui/home/").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn home_lists_a_registered_repository() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("home_lists_a_registered_repository");
    };
    register_repo(&db, "widget", "https://example.test/widget.git").await;
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get().uri("/ui/home").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("tests/widget"));
}

#[actix_web::test]
async fn project_page_renders_ok_for_a_known_project() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("project_page_renders_ok_for_a_known_project");
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
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri(&format!("/ui/projects/{}", project.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn project_page_reports_not_found_for_an_unknown_project() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("project_page_reports_not_found_for_an_unknown_project");
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/projects/does-not-exist")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn home_state_renders_the_runs_fragment() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("home_state_renders_the_runs_fragment");
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/home/state")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn home_state_scoped_to_a_project_renders_ok() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("home_state_scoped_to_a_project_renders_ok");
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
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri(&format!("/ui/home/state?project={}", project.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn run_now_for_an_unknown_repo_still_renders_the_runs_fragment() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "run_now_for_an_unknown_repo_still_renders_the_runs_fragment",
        );
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::post()
        .uri("/ui/home/repos/does-not-exist/run")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    // `trigger_manual`'s error is logged and swallowed, not surfaced as a
    // failed response - the fragment still renders.
    assert_eq!(resp.status(), StatusCode::OK);
}
