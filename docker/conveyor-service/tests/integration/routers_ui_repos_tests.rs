//! HTTP-level coverage for `routers/ui/pages/repos.rs`'s route handlers
//! (`list_page`, `edit_page`, `create_repo`, `save_repo`, `delete_repo`) -
//! the DB-touching orchestration the crate's own `routers_ui_repos_tests.rs`
//! deliberately leaves out, covering only the pure render helpers there.
//! `JwtConfig::for_tests()` has `auth_enabled: false`, so `get_user_from_req`
//! synthesizes an all-access actor and every write-grant check passes.

use crate::support::{self, database, register_repo};
use conveyor_service::scheduler::projects::{self, NewProject};
use http::{Method, StatusCode};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;

async fn app(
    db: Db,
    config: JwtConfig,
) -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    conveyor_service::routers::ui::register_routes();
    support::app(db, config).await
}

#[tokio::test]
async fn list_page_renders_ok_with_no_repositories() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("list_page_renders_ok_with_no_repositories");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(Method::GET, "/ui/repos", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn list_page_lists_a_registered_repository_and_offers_a_create_panel() {
    let Some((db, _guard)) = database().await else {
        return support::skipped(
            "list_page_lists_a_registered_repository_and_offers_a_create_panel",
        );
    };
    register_repo(&db, "widget", "https://example.test/widget.git").await;
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(Method::GET, "/ui/repos", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = support::body_text(resp).await;
    assert!(html.contains("tests/widget"));
    assert!(html.contains("ui_repos_add_title"));
}

#[tokio::test]
async fn edit_page_renders_ok_for_a_known_repository() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("edit_page_renders_ok_for_a_known_repository");
    };
    register_repo(&db, "widget", "https://example.test/widget.git").await;
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/repos/tests/widget/edit",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = support::body_text(resp).await;
    assert!(html.contains("widget"));
}

#[tokio::test]
async fn edit_page_reports_not_found_for_an_unknown_repository() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("edit_page_reports_not_found_for_an_unknown_repository");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/repos/nobody/nothing/edit",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_repo_rejects_an_empty_owner_or_name() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("create_repo_rejects_an_empty_owner_or_name");
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
        .call(support::form_req(
            Method::POST,
            "/ui/repos",
            &[
                ("owner", ""),
                ("name", ""),
                ("clone_url", "https://example.test/x.git"),
                ("project_id", project.id.as_str()),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let (headers, _) = support::parts(resp).await;
    assert!(support::location(&headers).contains("err=owner_name_empty"));
}

#[tokio::test]
async fn create_repo_rejects_a_malformed_clone_url() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("create_repo_rejects_a_malformed_clone_url");
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
        .call(support::form_req(
            Method::POST,
            "/ui/repos",
            &[
                ("owner", "tests"),
                ("name", "widget"),
                ("clone_url", "-malicious"),
                ("project_id", project.id.as_str()),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let (headers, _) = support::parts(resp).await;
    assert!(support::location(&headers).contains("err=bad_clone_url"));
}

#[tokio::test]
async fn create_repo_registers_a_valid_repository_and_redirects_to_the_list() {
    let Some((db, _guard)) = database().await else {
        return support::skipped(
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
    let (app, container) = app(db.clone(), JwtConfig::for_tests()).await;

    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/repos",
            &[
                ("owner", "tests"),
                ("name", "widget"),
                ("clone_url", "https://example.test/widget.git"),
                ("project_id", project.id.as_str()),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let (headers, _) = support::parts(resp).await;
    assert!(support::location(&headers).contains("ok=created"));

    let repos = conveyor_service::scheduler::repos::list(&db).await.unwrap();
    assert_eq!(repos.len(), 1);
    // `JwtConfig::for_tests()`'s auth bypass synthesizes an actor literally
    // named "admin" (see `routers::ui::common::actor`'s doc comment), not
    // `TEST_USER` - that constant only names the row `database()` seeds into
    // `auth.users` for tests that don't go through the HTTP auth bypass.
    assert_eq!(repos[0].registered_by, "admin");
}

#[tokio::test]
async fn save_repo_updates_an_existing_repository() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("save_repo_updates_an_existing_repository");
    };
    let repo = register_repo(&db, "widget", "https://example.test/widget.git").await;
    let (app, container) = app(db.clone(), JwtConfig::for_tests()).await;

    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/repos/tests/widget/edit",
            &[
                ("owner", "tests"),
                ("name", "widget"),
                ("clone_url", "https://example.test/widget-renamed.git"),
                ("project_id", repo.project_id.as_str()),
                ("enabled", "on"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let (headers, _) = support::parts(resp).await;
    assert!(support::location(&headers).contains("ok=saved"));

    let updated = conveyor_service::scheduler::repos::find_by_owner_name(&db, "tests", "widget")
        .await
        .unwrap()
        .expect("repo still exists");
    assert_eq!(updated.clone_url, "https://example.test/widget-renamed.git");
}

#[tokio::test]
async fn save_repo_redirects_home_for_an_unknown_repository() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("save_repo_redirects_home_for_an_unknown_repository");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/repos/nobody/nothing/edit",
            &[
                ("owner", "nobody"),
                ("name", "nothing"),
                ("clone_url", "https://example.test/x.git"),
                ("project_id", "does-not-exist"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let (headers, _) = support::parts(resp).await;
    assert!(support::location(&headers).contains("err=not_found"));
}

#[tokio::test]
async fn delete_repo_removes_a_registered_repository() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("delete_repo_removes_a_registered_repository");
    };
    register_repo(&db, "widget", "https://example.test/widget.git").await;
    let (app, container) = app(db.clone(), JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::POST,
            "/ui/repos/tests/widget/delete",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let (headers, _) = support::parts(resp).await;
    assert!(support::location(&headers).contains("ok=deleted"));

    let repos = conveyor_service::scheduler::repos::list(&db).await.unwrap();
    assert!(repos.is_empty());
}

#[tokio::test]
async fn delete_repo_redirects_home_for_an_unknown_repository() {
    let Some((db, _guard)) = database().await else {
        return support::skipped("delete_repo_redirects_home_for_an_unknown_repository");
    };
    let (app, container) = app(db, JwtConfig::for_tests()).await;

    let resp = app
        .call(support::req(
            Method::POST,
            "/ui/repos/nobody/nothing/delete",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let (headers, _) = support::parts(resp).await;
    assert!(support::location(&headers).contains("err=not_found"));
}
