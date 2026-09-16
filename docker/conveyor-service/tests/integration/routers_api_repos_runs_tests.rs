//! HTTP-layer coverage for `routers::api::repos` and `routers::api::runs`'
//! read/list/update/cancel/logs handlers - the JSON API mirror of what
//! `routers_ui_repos_tests.rs` covers for the browser pages.

use crate::support::{database, json_body, json_req, req, skipped};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::Trigger;
use conveyor_service::providers::Providers;
use conveyor_service::routers::api;
use conveyor_service::scheduler::projects::{self, NewProject};
use conveyor_service::scheduler::queue::{self, NewRun};
use conveyor_service::scheduler::repos::{self, NewRepo};
use http::{Method, StatusCode};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::{Database, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use std::sync::Arc;

/// `routers::api::Actor` resolves to the synthetic `"admin"` identity
/// `get_user_from_req` returns with `JwtConfig::for_tests()`'s `auth_enabled`
/// off (the default) whenever the container actually carries a `JwtConfig`,
/// which it does here - unlike the old actix version's test harness, which
/// never registered one as `app_data` and so fell back to a different,
/// incidental `"dev"` literal instead. Every write route stamps that name
/// into a `created_by`/`registered_by` foreign key into `auth.users` -
/// `database()` seeds `TEST_USER`, not `"admin"`, so any test performing a
/// write must seed this too.
async fn seed_admin_user(db: &Db) {
    db.execute("INSERT INTO auth.users (username, password, roles) VALUES ('admin', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING").await.expect("seed the admin user");
}

async fn app(db: Db) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    seed_admin_user(&db).await;
    api::register_routes();
    let container = ContainerBuilder::new()
        .provide(db)
        .provide(JwtConfig::for_tests())
        .provide(ConveyorConfig::default())
        .provide_arc(Arc::new(Providers::from_env()))
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

async fn seed_project(db: &Db) -> String {
    projects::create(
        db,
        &NewProject {
            name: "root".to_string(),
            parent_id: None,
        },
    )
    .await
    .expect("create the project")
    .id
}

async fn seed_repo(db: &Db, project_id: &str) -> conveyor_service::domain::Repo {
    repos::create(
        db,
        &NewRepo {
            provider: conveyor_service::domain::Provider::Generic,
            owner: "tests".to_string(),
            name: "widget".to_string(),
            clone_url: "https://example.test/widget.git".to_string(),
            default_branch: "master".to_string(),
            registered_by: "admin".to_string(),
            project_id: project_id.to_string(),
        },
    )
    .await
    .expect("create the repo")
}

// ---------------------------------------------------------------------------
// repos
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_rejects_an_unknown_provider() {
    let Some((db, _guard)) = database().await else {
        return skipped("register_rejects_an_unknown_provider");
    };
    let project_id = seed_project(&db).await;
    let (app, container) = app(db).await;

    let resp = app.call(json_req(Method::POST, "/api/v1/repos", serde_json::json!({ "provider": "not-a-provider", "owner": "tests", "name": "widget", "clone_url": "https://example.test/widget.git", "project_id": project_id }), &container)).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_creates_a_repo_when_valid() {
    let Some((db, _guard)) = database().await else {
        return skipped("register_creates_a_repo_when_valid");
    };
    let project_id = seed_project(&db).await;
    let (app, container) = app(db).await;

    let resp = app.call(json_req(Method::POST, "/api/v1/repos", serde_json::json!({ "owner": "tests", "name": "widget", "clone_url": "https://example.test/widget.git", "project_id": project_id }), &container)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn list_reports_every_registered_repo() {
    let Some((db, _guard)) = database().await else {
        return skipped("list_reports_every_registered_repo");
    };
    let project_id = seed_project(&db).await;
    seed_repo(&db, &project_id).await;
    let (app, container) = app(db).await;

    let resp = app
        .call(req(Method::GET, "/api/v1/repos", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body.as_array().expect("array").len(), 1);
}

#[tokio::test]
async fn read_reports_not_found_for_an_unknown_id() {
    let Some((db, _guard)) = database().await else {
        return skipped("read_reports_not_found_for_an_unknown_id");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(req(Method::GET, "/api/v1/repos/does-not-exist", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn read_returns_a_known_repo() {
    let Some((db, _guard)) = database().await else {
        return skipped("read_returns_a_known_repo");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/repos/{}", repo.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn update_rejects_an_empty_name() {
    let Some((db, _guard)) = database().await else {
        return skipped("update_rejects_an_empty_name");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::PATCH,
            &format!("/api/v1/repos/{}", repo.id),
            serde_json::json!({ "name": "   " }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_applies_a_valid_partial_change() {
    let Some((db, _guard)) = database().await else {
        return skipped("update_applies_a_valid_partial_change");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::PATCH,
            &format!("/api/v1/repos/{}", repo.id),
            serde_json::json!({ "default_branch": "develop" }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["default_branch"], "develop");
    // Untouched fields survive the partial update.
    assert_eq!(body["owner"], "tests");
}

#[tokio::test]
async fn set_enabled_toggles_the_flag() {
    let Some((db, _guard)) = database().await else {
        return skipped("set_enabled_toggles_the_flag");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/repos/{}/enabled", repo.id),
            serde_json::json!({ "enabled": false }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["enabled"], false);
}

#[tokio::test]
async fn remove_deletes_a_known_repo_then_404s_on_retry() {
    let Some((db, _guard)) = database().await else {
        return skipped("remove_deletes_a_known_repo_then_404s_on_retry");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/repos/{}", repo.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/repos/{}", repo.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// runs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn runs_list_is_empty_with_no_runs_queued() {
    let Some((db, _guard)) = database().await else {
        return skipped("runs_list_is_empty_with_no_runs_queued");
    };
    let (app, container) = app(db).await;

    let resp = app.call(req(Method::GET, "/api/v1/runs", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert!(body.as_array().expect("array").is_empty());
}

#[tokio::test]
async fn runs_list_scoped_to_an_unknown_repo_is_not_found() {
    let Some((db, _guard)) = database().await else {
        return skipped("runs_list_scoped_to_an_unknown_repo_is_not_found");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::GET,
            "/api/v1/runs?repo_id=does-not-exist",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn runs_list_reports_a_queued_run_scoped_to_its_repo() {
    let Some((db, _guard)) = database().await else {
        return skipped("runs_list_reports_a_queued_run_scoped_to_its_repo");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
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
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/runs?repo_id={}", repo.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body.as_array().expect("array").len(), 1);
}

#[tokio::test]
async fn runs_read_reports_not_found_for_an_unknown_run() {
    let Some((db, _guard)) = database().await else {
        return skipped("runs_read_reports_not_found_for_an_unknown_run");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(req(Method::GET, "/api/v1/runs/does-not-exist", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn runs_read_returns_the_run_with_jobs_and_artifacts() {
    let Some((db, _guard)) = database().await else {
        return skipped("runs_read_returns_the_run_with_jobs_and_artifacts");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
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
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/runs/{}", enqueued.run().id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["jobs"].as_array().unwrap().len(), 0);
    assert_eq!(body["artifacts"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn runs_cancel_reports_not_found_for_an_unknown_run() {
    let Some((db, _guard)) = database().await else {
        return skipped("runs_cancel_reports_not_found_for_an_unknown_run");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::POST,
            "/api/v1/runs/does-not-exist/cancel",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn runs_cancel_accepts_a_queued_run() {
    let Some((db, _guard)) = database().await else {
        return skipped("runs_cancel_accepts_a_queued_run");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
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
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::POST,
            &format!("/api/v1/runs/{}/cancel", enqueued.run().id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn job_logs_reports_not_found_for_an_unknown_job() {
    let Some((db, _guard)) = database().await else {
        return skipped("job_logs_reports_not_found_for_an_unknown_job");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::GET,
            "/api/v1/jobs/does-not-exist/logs",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
