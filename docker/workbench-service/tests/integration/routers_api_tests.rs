//! HTTP-layer coverage for `routers::api` - projects, issues, comments,
//! labels and issue-links - against a real Postgres. Auth is left disabled
//! (`JwtConfig::for_tests()`), the same as `routers_api_authz_tests.rs`
//! covers separately for the claims-driven branches: every `can_on_project`/
//! `can_unscoped` check here takes the disabled-auth bypass, so these tests
//! are about validation, not-found handling and the success path shape.

use crate::support::database;
use actix_web::http::StatusCode;
use actix_web::{App, test as actix_test, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use serde_json::json;
use workbench_service::domain::project::{self, NewProject};
use workbench_service::routers::api;

macro_rules! app {
    ($db:expr) => {
        actix_test::init_service(
            App::new()
                .app_data(web::Data::new($db))
                .service(api::scope(JwtConfig::for_tests())),
        )
        .await
    };
}

async fn seed_project(db: &Db, key: &str) -> project::Project {
    project::create(
        db,
        &NewProject {
            key: key.to_string(),
            name: format!("Project {key}"),
            description: None,
        },
    )
    .await
    .expect("create the project")
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn create_project_rejects_empty_key_or_name() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_project_rejects_empty_key_or_name");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/projects")
        .set_json(json!({"key": "  ", "name": "Anything"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn create_and_read_a_project_round_trips() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_and_read_a_project_round_trips");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/projects")
        .set_json(json!({"key": "WB", "name": "Workbench"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created: serde_json::Value = actix_test::read_body_json(resp).await;
    let id = created["id"].as_str().unwrap().to_string();

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/projects/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let read: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(read["key"], "WB");
}

#[actix_web::test]
async fn read_a_missing_project_is_404() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("read_a_missing_project_is_404");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/projects/no-such-id")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn list_projects_returns_every_project_when_auth_is_disabled() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "list_projects_returns_every_project_when_auth_is_disabled",
        );
    };
    seed_project(&db, "A").await;
    seed_project(&db, "B").await;
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/projects")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let list: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
}

#[actix_web::test]
async fn update_project_rejects_empty_name_then_applies_a_valid_one() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "update_project_rejects_empty_name_then_applies_a_valid_one",
        );
    };
    let project = seed_project(&db, "UP").await;
    let app = app!(db);

    let req = actix_test::TestRequest::put()
        .uri(&format!("/api/v1/projects/{}", project.id))
        .set_json(json!({"name": ""}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = actix_test::TestRequest::put()
        .uri(&format!("/api/v1/projects/{}", project.id))
        .set_json(json!({"name": "Renamed", "description": "new"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let updated: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(updated["name"], "Renamed");
}

#[actix_web::test]
async fn update_a_missing_project_is_404() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("update_a_missing_project_is_404");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::put()
        .uri("/api/v1/projects/no-such-id")
        .set_json(json!({"name": "Renamed"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn remove_a_project_then_a_second_remove_is_404() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("remove_a_project_then_a_second_remove_is_404");
    };
    let project = seed_project(&db, "RM").await;
    let app = app!(db);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/projects/{}", project.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/projects/{}", project.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Issues
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn create_issue_rejects_empty_title() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_issue_rejects_empty_title");
    };
    let project = seed_project(&db, "IS").await;
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/projects/{}/issues", project.id))
        .set_json(json!({"title": "   "}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn create_list_read_update_transition_and_remove_an_issue() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_list_read_update_transition_and_remove_an_issue");
    };
    let project = seed_project(&db, "IF").await;
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/projects/{}/issues", project.id))
        .set_json(json!({"title": "Do the thing"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created: serde_json::Value = actix_test::read_body_json(resp).await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["kind"], "task");
    assert_eq!(created["priority"], "medium");

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/projects/{}/issues", project.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let list: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/issues/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let req = actix_test::TestRequest::put()
        .uri(&format!("/api/v1/issues/{id}"))
        .set_json(json!({
            "title": "  ",
            "kind": "task",
            "priority": "high",
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = actix_test::TestRequest::put()
        .uri(&format!("/api/v1/issues/{id}"))
        .set_json(json!({
            "title": "Do the updated thing",
            "kind": "bug",
            "priority": "high",
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let updated: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(updated["title"], "Do the updated thing");

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/issues/{id}/transition"))
        .set_json(json!({"status": "not-a-status"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/issues/{id}/transition"))
        .set_json(json!({"status": "in-progress"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let transitioned: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(transitioned["status"], "in-progress");

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/issues/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/issues/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn issue_actions_on_a_missing_issue_are_404() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("issue_actions_on_a_missing_issue_are_404");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::put()
        .uri("/api/v1/issues/no-such-id")
        .set_json(json!({"title": "x", "kind": "task", "priority": "medium"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/issues/no-such-id/transition")
        .set_json(json!({"status": "todo"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let req = actix_test::TestRequest::delete()
        .uri("/api/v1/issues/no-such-id")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Labels (project-scoped, and attach/detach on an issue)
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn create_list_attach_and_detach_a_label() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_list_attach_and_detach_a_label");
    };
    let project = seed_project(&db, "LB").await;
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/projects/{}/labels", project.id))
        .set_json(json!({"name": "  "}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/projects/{}/labels", project.id))
        .set_json(json!({"name": "bug"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let label: serde_json::Value = actix_test::read_body_json(resp).await;
    let label_id = label["id"].as_str().unwrap().to_string();

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/projects/{}/labels", project.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let list: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/projects/{}/issues", project.id))
        .set_json(json!({"title": "Labeled"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    let issue: serde_json::Value = actix_test::read_body_json(resp).await;
    let issue_id = issue["id"].as_str().unwrap().to_string();

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/issues/{issue_id}/labels/{label_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/issues/{issue_id}/labels"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let labels: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(labels.as_array().unwrap().len(), 1);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/issues/{issue_id}/labels/{label_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/labels/{label_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/labels/{label_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn create_rejects_empty_body_then_lists_and_removes_a_comment() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "create_rejects_empty_body_then_lists_and_removes_a_comment",
        );
    };
    let project = seed_project(&db, "CM").await;
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/projects/{}/issues", project.id))
        .set_json(json!({"title": "Commented"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    let issue: serde_json::Value = actix_test::read_body_json(resp).await;
    let issue_id = issue["id"].as_str().unwrap().to_string();

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/issues/{issue_id}/comments"))
        .set_json(json!({"body": "   "}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/issues/no-such-issue/comments")
        .set_json(json!({"body": "hi"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/issues/{issue_id}/comments"))
        .set_json(json!({"body": "First comment"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let comment: serde_json::Value = actix_test::read_body_json(resp).await;
    let comment_id = comment["id"].as_str().unwrap().to_string();

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/issues/{issue_id}/comments"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let list: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/comments/{comment_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/comments/{comment_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Issue links
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn issue_link_validation_then_create_list_and_remove() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("issue_link_validation_then_create_list_and_remove");
    };
    let project = seed_project(&db, "LK").await;
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/projects/{}/issues", project.id))
        .set_json(json!({"title": "Blocker"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    let blocker: serde_json::Value = actix_test::read_body_json(resp).await;
    let blocker_id = blocker["id"].as_str().unwrap().to_string();

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/projects/{}/issues", project.id))
        .set_json(json!({"title": "Blocked"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    let blocked: serde_json::Value = actix_test::read_body_json(resp).await;
    let blocked_id = blocked["id"].as_str().unwrap().to_string();

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/issues/{blocker_id}/links"))
        .set_json(json!({"linked_issue_id": blocked_id, "kind": "not-a-kind"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/issues/{blocker_id}/links"))
        .set_json(json!({"linked_issue_id": blocker_id, "kind": "blocks"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/api/v1/issues/{blocker_id}/links"))
        .set_json(json!({"linked_issue_id": blocked_id, "kind": "blocks"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let link: serde_json::Value = actix_test::read_body_json(resp).await;
    let link_id = link["id"].as_str().unwrap().to_string();

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/issues/{blocker_id}/links"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/issue-links/{link_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/issue-links/{link_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
