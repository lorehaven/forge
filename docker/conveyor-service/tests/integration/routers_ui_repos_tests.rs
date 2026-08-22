//! HTTP-level coverage for `routers/ui/pages/repos.rs`'s route handlers
//! (`list_page`, `edit_page`, `create_repo`, `save_repo`, `delete_repo`) -
//! the DB-touching orchestration the crate's own `routers_ui_repos_tests.rs`
//! deliberately leaves out, covering only the pure render helpers there.
//! `JwtConfig::for_tests()` has `auth_enabled: false`, so `get_user_from_req`
//! synthesizes an all-access actor and every write-grant check passes.

use crate::support::{database, register_repo};
use actix_web::http::StatusCode;
use actix_web::web::Data;
use actix_web::{App, test as actix_test};
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
        .service(ui::scope(config))
}

fn location_of(resp: &actix_web::dev::ServiceResponse) -> String {
    resp.headers()
        .get("Location")
        .expect("a Location header")
        .to_str()
        .unwrap()
        .to_string()
}

#[actix_web::test]
async fn list_page_renders_ok_with_no_repositories() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("list_page_renders_ok_with_no_repositories");
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get().uri("/ui/repos").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn list_page_lists_a_registered_repository_and_offers_a_create_panel() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "list_page_lists_a_registered_repository_and_offers_a_create_panel",
        );
    };
    register_repo(&db, "widget", "https://example.test/widget.git").await;
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get().uri("/ui/repos").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("tests/widget"));
    assert!(html.contains("ui_repos_add_title"));
}

#[actix_web::test]
async fn edit_page_renders_ok_for_a_known_repository() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("edit_page_renders_ok_for_a_known_repository");
    };
    register_repo(&db, "widget", "https://example.test/widget.git").await;
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/repos/tests/widget/edit")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("widget"));
}

#[actix_web::test]
async fn edit_page_reports_not_found_for_an_unknown_repository() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("edit_page_reports_not_found_for_an_unknown_repository");
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::get()
        .uri("/ui/repos/nobody/nothing/edit")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn create_repo_rejects_an_empty_owner_or_name() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_repo_rejects_an_empty_owner_or_name");
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

    let req = actix_test::TestRequest::post()
        .uri("/ui/repos")
        .set_form([
            ("owner", ""),
            ("name", ""),
            ("clone_url", "https://example.test/x.git"),
            ("project_id", project.id.as_str()),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("err=owner_name_empty"));
}

#[actix_web::test]
async fn create_repo_rejects_a_malformed_clone_url() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_repo_rejects_a_malformed_clone_url");
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

    let req = actix_test::TestRequest::post()
        .uri("/ui/repos")
        .set_form([
            ("owner", "tests"),
            ("name", "widget"),
            ("clone_url", "-malicious"),
            ("project_id", project.id.as_str()),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("err=bad_clone_url"));
}

#[actix_web::test]
async fn create_repo_registers_a_valid_repository_and_redirects_to_the_list() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "create_repo_registers_a_valid_repository_and_redirects_to_the_list",
        );
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
    let app = actix_test::init_service(app_with(db.clone(), JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::post()
        .uri("/ui/repos")
        .set_form([
            ("owner", "tests"),
            ("name", "widget"),
            ("clone_url", "https://example.test/widget.git"),
            ("project_id", project.id.as_str()),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("ok=created"));

    let repos = conveyor_service::scheduler::repos::list(&db).await.unwrap();
    assert_eq!(repos.len(), 1);
    // `JwtConfig::for_tests()`'s auth bypass synthesizes an actor literally
    // named "admin" (see `routers::ui::common::actor`'s doc comment), not
    // `TEST_USER` - that constant only names the row `database()` seeds into
    // `auth.users` for tests that don't go through the HTTP auth bypass.
    assert_eq!(repos[0].registered_by, "admin");
}

#[actix_web::test]
async fn save_repo_updates_an_existing_repository() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("save_repo_updates_an_existing_repository");
    };
    let repo = register_repo(&db, "widget", "https://example.test/widget.git").await;
    let app = actix_test::init_service(app_with(db.clone(), JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::post()
        .uri("/ui/repos/tests/widget/edit")
        .set_form([
            ("owner", "tests"),
            ("name", "widget"),
            ("clone_url", "https://example.test/widget-renamed.git"),
            ("project_id", repo.project_id.as_str()),
            ("enabled", "on"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("ok=saved"));

    let updated = conveyor_service::scheduler::repos::find_by_owner_name(&db, "tests", "widget")
        .await
        .unwrap()
        .expect("repo still exists");
    assert_eq!(updated.clone_url, "https://example.test/widget-renamed.git");
}

#[actix_web::test]
async fn save_repo_redirects_home_for_an_unknown_repository() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("save_repo_redirects_home_for_an_unknown_repository");
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::post()
        .uri("/ui/repos/nobody/nothing/edit")
        .set_form([
            ("owner", "nobody"),
            ("name", "nothing"),
            ("clone_url", "https://example.test/x.git"),
            ("project_id", "does-not-exist"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("err=not_found"));
}

#[actix_web::test]
async fn delete_repo_removes_a_registered_repository() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("delete_repo_removes_a_registered_repository");
    };
    register_repo(&db, "widget", "https://example.test/widget.git").await;
    let app = actix_test::init_service(app_with(db.clone(), JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::post()
        .uri("/ui/repos/tests/widget/delete")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("ok=deleted"));

    let repos = conveyor_service::scheduler::repos::list(&db).await.unwrap();
    assert!(repos.is_empty());
}

#[actix_web::test]
async fn delete_repo_redirects_home_for_an_unknown_repository() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("delete_repo_redirects_home_for_an_unknown_repository");
    };
    let app = actix_test::init_service(app_with(db, JwtConfig::for_tests())).await;

    let req = actix_test::TestRequest::post()
        .uri("/ui/repos/nobody/nothing/delete")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("err=not_found"));
}
