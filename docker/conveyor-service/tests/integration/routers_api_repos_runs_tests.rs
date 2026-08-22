//! HTTP-layer coverage for `routers::api::repos` and `routers::api::runs`'
//! read/list/update/cancel/logs handlers - the JSON API mirror of what
//! `routers_ui_repos_tests.rs` covers for the browser pages.

use crate::support::{database, skipped};
use actix_web::http::StatusCode;
use actix_web::{App, test as actix_test, web};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::Trigger;
use conveyor_service::providers::Providers;
use conveyor_service::routers::api;
use conveyor_service::scheduler::projects::{self, NewProject};
use conveyor_service::scheduler::queue::{self, NewRun};
use conveyor_service::scheduler::repos::{self, NewRepo};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::{Database, Db};

/// `routers::api::actor` falls back to the literal user `"dev"` with auth
/// disabled, and every write route stamps that name into a `created_by`/
/// `registered_by` foreign key into `auth.users` - `database()` seeds
/// `TEST_USER`, not `"dev"`, so any test performing a write must seed this.
async fn seed_dev_user(db: &Db) {
    db.execute(
        "INSERT INTO auth.users (username, password, roles) \
         VALUES ('dev', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING",
    )
    .await
    .expect("seed the dev user");
}

macro_rules! app {
    ($db:expr) => {{
        let db = $db;
        seed_dev_user(&db).await;
        actix_test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(Providers::from_env()))
                .app_data(web::Data::new(ConveyorConfig::default()))
                .service(api::scope(JwtConfig::for_tests())),
        )
        .await
    }};
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
            registered_by: "dev".to_string(),
            project_id: project_id.to_string(),
        },
    )
    .await
    .expect("create the repo")
}

// ---------------------------------------------------------------------------
// repos
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn register_rejects_an_unknown_provider() {
    let Some((db, _guard)) = database().await else {
        return skipped("register_rejects_an_unknown_provider");
    };
    let project_id = seed_project(&db).await;
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/repos")
        .set_json(serde_json::json!({
            "provider": "not-a-provider",
            "owner": "tests",
            "name": "widget",
            "clone_url": "https://example.test/widget.git",
            "project_id": project_id,
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn register_creates_a_repo_when_valid() {
    let Some((db, _guard)) = database().await else {
        return skipped("register_creates_a_repo_when_valid");
    };
    let project_id = seed_project(&db).await;
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/repos")
        .set_json(serde_json::json!({
            "owner": "tests",
            "name": "widget",
            "clone_url": "https://example.test/widget.git",
            "project_id": project_id,
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[actix_web::test]
async fn list_reports_every_registered_repo() {
    let Some((db, _guard)) = database().await else {
        return skipped("list_reports_every_registered_repo");
    };
    let project_id = seed_project(&db).await;
    seed_repo(&db, &project_id).await;
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/repos")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Vec<serde_json::Value> = actix_test::read_body_json(resp).await;
    assert_eq!(body.len(), 1);
}

#[actix_web::test]
async fn read_reports_not_found_for_an_unknown_id() {
    let Some((db, _guard)) = database().await else {
        return skipped("read_reports_not_found_for_an_unknown_id");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/repos/does-not-exist")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn read_returns_a_known_repo() {
    let Some((db, _guard)) = database().await else {
        return skipped("read_returns_a_known_repo");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/repos/{}", repo.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn update_rejects_an_empty_name() {
    let Some((db, _guard)) = database().await else {
        return skipped("update_rejects_an_empty_name");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
    let app = app!(db);

    let req = actix_test::TestRequest::patch()
        .uri(&format!("/api/v1/repos/{}", repo.id))
        .set_json(serde_json::json!({ "name": "   " }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn update_applies_a_valid_partial_change() {
    let Some((db, _guard)) = database().await else {
        return skipped("update_applies_a_valid_partial_change");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
    let app = app!(db);

    let req = actix_test::TestRequest::patch()
        .uri(&format!("/api/v1/repos/{}", repo.id))
        .set_json(serde_json::json!({ "default_branch": "develop" }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["default_branch"], "develop");
    // Untouched fields survive the partial update.
    assert_eq!(body["owner"], "tests");
}

#[actix_web::test]
async fn set_enabled_toggles_the_flag() {
    let Some((db, _guard)) = database().await else {
        return skipped("set_enabled_toggles_the_flag");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/repos/{}/enabled", repo.id))
        .set_json(serde_json::json!({ "enabled": false }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["enabled"], false);
}

#[actix_web::test]
async fn remove_deletes_a_known_repo_then_404s_on_retry() {
    let Some((db, _guard)) = database().await else {
        return skipped("remove_deletes_a_known_repo_then_404s_on_retry");
    };
    let project_id = seed_project(&db).await;
    let repo = seed_repo(&db, &project_id).await;
    let app = app!(db);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/repos/{}", repo.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/repos/{}", repo.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// runs
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn runs_list_is_empty_with_no_runs_queued() {
    let Some((db, _guard)) = database().await else {
        return skipped("runs_list_is_empty_with_no_runs_queued");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/runs")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Vec<serde_json::Value> = actix_test::read_body_json(resp).await;
    assert!(body.is_empty());
}

#[actix_web::test]
async fn runs_list_scoped_to_an_unknown_repo_is_not_found() {
    let Some((db, _guard)) = database().await else {
        return skipped("runs_list_scoped_to_an_unknown_repo_is_not_found");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/runs?repo_id=does-not-exist")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
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
        },
    )
    .await
    .expect("enqueue");
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/runs?repo_id={}", repo.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Vec<serde_json::Value> = actix_test::read_body_json(resp).await;
    assert_eq!(body.len(), 1);
}

#[actix_web::test]
async fn runs_read_reports_not_found_for_an_unknown_run() {
    let Some((db, _guard)) = database().await else {
        return skipped("runs_read_reports_not_found_for_an_unknown_run");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/runs/does-not-exist")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
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
        },
    )
    .await
    .expect("enqueue");
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/runs/{}", enqueued.run().id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["jobs"].as_array().unwrap().len(), 0);
    assert_eq!(body["artifacts"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn runs_cancel_reports_not_found_for_an_unknown_run() {
    let Some((db, _guard)) = database().await else {
        return skipped("runs_cancel_reports_not_found_for_an_unknown_run");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/runs/does-not-exist/cancel")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
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
        },
    )
    .await
    .expect("enqueue");
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/runs/{}/cancel", enqueued.run().id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
}

#[actix_web::test]
async fn job_logs_reports_not_found_for_an_unknown_job() {
    let Some((db, _guard)) = database().await else {
        return skipped("job_logs_reports_not_found_for_an_unknown_job");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/jobs/does-not-exist/logs")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
