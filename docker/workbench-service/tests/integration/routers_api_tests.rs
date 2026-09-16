//! HTTP-layer coverage for `routers::api` - projects, issues, comments,
//! labels and issue-links - against a real Postgres. Auth is left disabled
//! (`JwtConfig::for_tests()`), the same as `routers_api_authz_tests.rs`
//! covers separately for the claims-driven branches: every `can_on_project`/
//! `can_unscoped` check here takes the disabled-auth bypass, so these tests
//! are about validation, not-found handling and the success path shape.

use crate::support::database;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use serde_json::json;
use std::sync::Arc;
use workbench_service::domain::project::{self, NewProject};
use workbench_service::routers::api;

async fn app(db: Db) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    api::register_routes();
    let container = ContainerBuilder::new()
        .provide(JwtConfig::for_tests())
        .provide(db)
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn req(method: Method, path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

fn json_req(
    method: Method,
    path: &str,
    body: serde_json::Value,
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
        container.clone(),
    )
}

async fn json_body(resp: quench_http::response::Response) -> serde_json::Value {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    serde_json::from_slice(&collected.to_bytes()).expect("valid json body")
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

#[tokio::test]
async fn create_project_rejects_empty_key_or_name() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_project_rejects_empty_key_or_name");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/projects",
            json!({"key": "  ", "name": "Anything"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_and_read_a_project_round_trips() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_and_read_a_project_round_trips");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/projects",
            json!({"key": "WB", "name": "Workbench"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created = json_body(resp).await;
    let id = created["id"].as_str().unwrap().to_string();

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/projects/{id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let read = json_body(resp).await;
    assert_eq!(read["key"], "WB");
}

#[tokio::test]
async fn read_a_missing_project_is_404() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("read_a_missing_project_is_404");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(req(Method::GET, "/api/v1/projects/no-such-id", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_projects_returns_every_project_when_auth_is_disabled() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "list_projects_returns_every_project_when_auth_is_disabled",
        );
    };
    seed_project(&db, "A").await;
    seed_project(&db, "B").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(req(Method::GET, "/api/v1/projects", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let list = json_body(resp).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn update_project_rejects_empty_name_then_applies_a_valid_one() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "update_project_rejects_empty_name_then_applies_a_valid_one",
        );
    };
    let project = seed_project(&db, "UP").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::PUT,
            &format!("/api/v1/projects/{}", project.id),
            json!({"name": ""}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .call(json_req(
            Method::PUT,
            &format!("/api/v1/projects/{}", project.id),
            json!({"name": "Renamed", "description": "new"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let updated = json_body(resp).await;
    assert_eq!(updated["name"], "Renamed");
}

#[tokio::test]
async fn update_a_missing_project_is_404() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("update_a_missing_project_is_404");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::PUT,
            "/api/v1/projects/no-such-id",
            json!({"name": "Renamed"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn remove_a_project_then_a_second_remove_is_404() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("remove_a_project_then_a_second_remove_is_404");
    };
    let project = seed_project(&db, "RM").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/projects/{}", project.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/projects/{}", project.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Issues
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_issue_rejects_empty_title() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_issue_rejects_empty_title");
    };
    let project = seed_project(&db, "IS").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/projects/{}/issues", project.id),
            json!({"title": "   "}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_list_read_update_transition_and_remove_an_issue() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_list_read_update_transition_and_remove_an_issue");
    };
    let project = seed_project(&db, "IF").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/projects/{}/issues", project.id),
            json!({"title": "Do the thing"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created = json_body(resp).await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["kind"], "task");
    assert_eq!(created["priority"], "medium");

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/projects/{}/issues", project.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let list = json_body(resp).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/issues/{id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .call(json_req(
            Method::PUT,
            &format!("/api/v1/issues/{id}"),
            json!({"title": "  ", "kind": "task", "priority": "high"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .call(json_req(
            Method::PUT,
            &format!("/api/v1/issues/{id}"),
            json!({"title": "Do the updated thing", "kind": "bug", "priority": "high"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let updated = json_body(resp).await;
    assert_eq!(updated["title"], "Do the updated thing");

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/issues/{id}/transition"),
            json!({"status": "not-a-status"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/issues/{id}/transition"),
            json!({"status": "in-progress"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let transitioned = json_body(resp).await;
    assert_eq!(transitioned["status"], "in-progress");

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/issues/{id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/issues/{id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn issue_actions_on_a_missing_issue_are_404() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("issue_actions_on_a_missing_issue_are_404");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::PUT,
            "/api/v1/issues/no-such-id",
            json!({"title": "x", "kind": "task", "priority": "medium"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/issues/no-such-id/transition",
            json!({"status": "todo"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let resp = app
        .call(req(Method::DELETE, "/api/v1/issues/no-such-id", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Labels (project-scoped, and attach/detach on an issue)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_list_attach_and_detach_a_label() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_list_attach_and_detach_a_label");
    };
    let project = seed_project(&db, "LB").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/projects/{}/labels", project.id),
            json!({"name": "  "}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/projects/{}/labels", project.id),
            json!({"name": "bug"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let label = json_body(resp).await;
    let label_id = label["id"].as_str().unwrap().to_string();

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/projects/{}/labels", project.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let list = json_body(resp).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/projects/{}/issues", project.id),
            json!({"title": "Labeled"}),
            &container,
        ))
        .await;
    let issue = json_body(resp).await;
    let issue_id = issue["id"].as_str().unwrap().to_string();

    let resp = app
        .call(req(
            Method::POST,
            &format!("/api/v1/issues/{issue_id}/labels/{label_id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/issues/{issue_id}/labels"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let labels = json_body(resp).await;
    assert_eq!(labels.as_array().unwrap().len(), 1);

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/issues/{issue_id}/labels/{label_id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/labels/{label_id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/labels/{label_id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_rejects_empty_body_then_lists_and_removes_a_comment() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "create_rejects_empty_body_then_lists_and_removes_a_comment",
        );
    };
    let project = seed_project(&db, "CM").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/projects/{}/issues", project.id),
            json!({"title": "Commented"}),
            &container,
        ))
        .await;
    let issue = json_body(resp).await;
    let issue_id = issue["id"].as_str().unwrap().to_string();

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/issues/{issue_id}/comments"),
            json!({"body": "   "}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/issues/no-such-issue/comments",
            json!({"body": "hi"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/issues/{issue_id}/comments"),
            json!({"body": "First comment"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let comment = json_body(resp).await;
    let comment_id = comment["id"].as_str().unwrap().to_string();

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/issues/{issue_id}/comments"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let list = json_body(resp).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/comments/{comment_id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/comments/{comment_id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Issue links
// ---------------------------------------------------------------------------

#[tokio::test]
async fn issue_link_validation_then_create_list_and_remove() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("issue_link_validation_then_create_list_and_remove");
    };
    let project = seed_project(&db, "LK").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/projects/{}/issues", project.id),
            json!({"title": "Blocker"}),
            &container,
        ))
        .await;
    let blocker = json_body(resp).await;
    let blocker_id = blocker["id"].as_str().unwrap().to_string();

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/projects/{}/issues", project.id),
            json!({"title": "Blocked"}),
            &container,
        ))
        .await;
    let blocked = json_body(resp).await;
    let blocked_id = blocked["id"].as_str().unwrap().to_string();

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/issues/{blocker_id}/links"),
            json!({"linked_issue_id": blocked_id, "kind": "not-a-kind"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/issues/{blocker_id}/links"),
            json!({"linked_issue_id": blocker_id, "kind": "blocks"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .call(json_req(
            Method::POST,
            &format!("/api/v1/issues/{blocker_id}/links"),
            json!({"linked_issue_id": blocked_id, "kind": "blocks"}),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let link = json_body(resp).await;
    let link_id = link["id"].as_str().unwrap().to_string();

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/issues/{blocker_id}/links"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/issue-links/{link_id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/issue-links/{link_id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
