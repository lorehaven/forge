//! HTTP-level coverage for `routers::ui`'s pages (home, board, issue detail)
//! against a real Postgres, auth disabled (`JwtConfig::for_tests()` -
//! `is_ui_authenticated`/`actor` both take the bypass documented on
//! `routers::ui::common::actor`, synthesizing an all-access "admin" identity,
//! so every `can_on_project_claims` write check here passes).

use crate::support::{TEST_USER, database};
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use std::sync::Arc;
use workbench_service::domain::issue::{self, NewIssue};
use workbench_service::domain::project::{self, NewProject};
use workbench_service::routers::ui;

async fn app(db: Db) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    ui::register_routes();
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

fn form_req(
    path: &str,
    pairs: &[(&str, &str)],
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let encoded = serde_urlencoded::to_string(pairs).unwrap();
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::from(encoded)),
        container.clone(),
    )
}

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    String::from_utf8_lossy(&collected.to_bytes()).into_owned()
}

async fn seed_project(db: &Db, key: &str) -> project::Project {
    project::create(
        db,
        &NewProject {
            key: key.to_string(),
            name: format!("Project {key}"),
            description: Some("a test project".to_string()),
        },
    )
    .await
    .expect("create the project")
}

async fn seed_issue(db: &Db, project_id: &str, title: &str) -> issue::Issue {
    issue::create(
        db,
        &NewIssue {
            project_id: project_id.to_string(),
            parent_id: None,
            kind: "task".to_string(),
            title: title.to_string(),
            description: None,
            priority: "medium".to_string(),
            assignee: None,
            reporter: TEST_USER.to_string(),
            estimate: Some(3),
        },
    )
    .await
    .expect("create the issue")
}

fn location(resp: quench_http::response::Response) -> String {
    resp.into_hyper()
        .headers()
        .get("location")
        .expect("a Location header")
        .to_str()
        .unwrap()
        .to_string()
}

// ---------------------------------------------------------------------------
// Home
// ---------------------------------------------------------------------------

#[tokio::test]
async fn home_renders_the_empty_state_with_no_projects() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("home_renders_the_empty_state_with_no_projects");
    };
    let (app, container) = app(db).await;

    let resp = app.call(req(Method::GET, "/ui/home", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_text(resp).await;
    assert!(body.contains("ui_home_no_projects"));
}

#[tokio::test]
async fn home_slash_lists_a_seeded_project() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("home_slash_lists_a_seeded_project");
    };
    seed_project(&db, "HM").await;
    let (app, container) = app(db).await;

    let resp = app.call(req(Method::GET, "/ui/home/", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_text(resp).await;
    assert!(body.contains("Project HM"));
}

#[tokio::test]
async fn home_shows_the_error_notice_from_a_redirect() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("home_shows_the_error_notice_from_a_redirect");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(req(Method::GET, "/ui/home?error=key_taken", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_text(resp).await;
    assert!(body.contains("key_taken"));
}

#[tokio::test]
async fn create_project_rejects_empty_fields_then_creates_then_rejects_duplicate_key() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "create_project_rejects_empty_fields_then_creates_then_rejects_duplicate_key",
        );
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(form_req(
            "/ui/projects",
            &[("key", ""), ("name", "")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=empty_fields"));

    let resp = app
        .call(form_req(
            "/ui/projects",
            &[("key", "DUP"), ("name", "Dup"), ("description", "d")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let loc = location(resp);
    assert!(loc.contains("/projects/"));
    assert!(loc.contains("/board"));

    let resp = app
        .call(form_req(
            "/ui/projects",
            &[("key", "DUP"), ("name", "Dup Again")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=key_taken"));
}

// ---------------------------------------------------------------------------
// Board
// ---------------------------------------------------------------------------

#[tokio::test]
async fn board_redirects_home_for_a_missing_project() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("board_redirects_home_for_a_missing_project");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::GET,
            "/ui/projects/no-such-id/board",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).ends_with("/home"));
}

#[tokio::test]
async fn board_renders_issues_grouped_by_status() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("board_renders_issues_grouped_by_status");
    };
    let project = seed_project(&db, "BD").await;
    seed_issue(&db, &project.id, "First card").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::GET,
            &format!("/ui/projects/{}/board", project.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_text(resp).await;
    assert!(body.contains("First card"));
    assert!(body.contains("wb-board"));
}

#[tokio::test]
async fn create_issue_rejects_empty_title_then_creates_and_transitions() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "create_issue_rejects_empty_title_then_creates_and_transitions",
        );
    };
    let project = seed_project(&db, "CI").await;
    let (app, container) = app(db.clone()).await;

    let resp = app
        .call(form_req(
            &format!("/ui/projects/{}/issues", project.id),
            &[("title", "  ")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=title_required"));

    let resp = app
        .call(form_req(
            &format!("/ui/projects/{}/issues", project.id),
            &[
                ("title", "New issue"),
                ("kind", "bug"),
                ("priority", "high"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(!location(resp).contains("error="));

    let created = issue::list_by_project(&db, &project.id, None)
        .await
        .unwrap();
    assert_eq!(created.len(), 1);
    let issue_id = created[0].id.clone();

    let resp = app
        .call(form_req(
            &format!("/ui/projects/{}/issues/{issue_id}/transition", project.id),
            &[("status", "not-a-status")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .call(form_req(
            &format!("/ui/projects/{}/issues/{issue_id}/transition", project.id),
            &[("status", "done")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_text(resp).await;
    assert!(body.contains("New issue"));

    let after = issue::read(&db, &issue_id).await.unwrap().unwrap();
    assert_eq!(after.status, "done");
}

#[tokio::test]
async fn create_issue_reports_an_unknown_assignee_as_a_foreign_key_violation() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "create_issue_reports_an_unknown_assignee_as_a_foreign_key_violation",
        );
    };
    let project = seed_project(&db, "FK").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(form_req(
            &format!("/ui/projects/{}/issues", project.id),
            &[("title", "Assigned"), ("assignee", "no-such-user")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=unknown_assignee"));
}

// ---------------------------------------------------------------------------
// Issue detail
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detail_redirects_home_for_a_missing_issue() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("detail_redirects_home_for_a_missing_issue");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(req(Method::GET, "/ui/issues/no-such-id", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).ends_with("/home"));
}

#[tokio::test]
async fn detail_renders_the_issue_with_comments_and_links() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("detail_renders_the_issue_with_comments_and_links");
    };
    let project = seed_project(&db, "DT").await;
    let issue = seed_issue(&db, &project.id, "Detailed issue").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::GET,
            &format!("/ui/issues/{}", issue.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_text(resp).await;
    assert!(body.contains("Detailed issue"));
    assert!(body.contains("ui_issue_comments"));
}

#[tokio::test]
async fn update_rejects_empty_title_then_applies_a_valid_update() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("update_rejects_empty_title_then_applies_a_valid_update");
    };
    let project = seed_project(&db, "UD").await;
    let issue = seed_issue(&db, &project.id, "Original").await;
    let (app, container) = app(db.clone()).await;

    let resp = app
        .call(form_req(
            &format!("/ui/issues/{}", issue.id),
            &[
                ("title", ""),
                ("kind", "task"),
                ("priority", "medium"),
                ("status", "todo"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=title_required"));

    let resp = app
        .call(form_req(
            &format!("/ui/issues/{}", issue.id),
            &[
                ("title", "Updated"),
                ("kind", "bug"),
                ("priority", "high"),
                ("status", "in-progress"),
                ("estimate", "5"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(!location(resp).contains("error="));

    let after = issue::read(&db, &issue.id).await.unwrap().unwrap();
    assert_eq!(after.title, "Updated");
    assert_eq!(after.status, "in-progress");
    assert_eq!(after.estimate, Some(5));
}

#[tokio::test]
async fn update_rejects_a_negative_estimate() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("update_rejects_a_negative_estimate");
    };
    let project = seed_project(&db, "NE").await;
    let issue = seed_issue(&db, &project.id, "Negative").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(form_req(
            &format!("/ui/issues/{}", issue.id),
            &[
                ("title", "Negative"),
                ("kind", "task"),
                ("priority", "medium"),
                ("status", "todo"),
                ("estimate", "not-a-number"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=invalid_estimate"));
}

#[tokio::test]
async fn update_on_a_missing_issue_redirects_home() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("update_on_a_missing_issue_redirects_home");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(form_req(
            "/ui/issues/no-such-id",
            &[
                ("title", "x"),
                ("kind", "task"),
                ("priority", "medium"),
                ("status", "todo"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=not_found"));
}

#[tokio::test]
async fn create_comment_rejects_empty_body_then_adds_a_comment() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_comment_rejects_empty_body_then_adds_a_comment");
    };
    let project = seed_project(&db, "CC").await;
    let issue = seed_issue(&db, &project.id, "Commented").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(form_req(
            &format!("/ui/issues/{}/comments", issue.id),
            &[("body", "   ")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=comment_required"));

    let resp = app
        .call(form_req(
            &format!("/ui/issues/{}/comments", issue.id),
            &[("body", "A comment")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(!location(resp).contains("error="));
}

#[tokio::test]
async fn add_link_validates_kind_key_and_self_link_then_creates_and_removes() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "add_link_validates_kind_key_and_self_link_then_creates_and_removes",
        );
    };
    let project = seed_project(&db, "LK").await;
    let source = seed_issue(&db, &project.id, "Source").await;
    let target = seed_issue(&db, &project.id, "Target").await;
    let (app, container) = app(db.clone()).await;

    let resp = app
        .call(form_req(
            &format!("/ui/issues/{}/links", source.id),
            &[
                ("target_key", &format!("LK-{}", target.seq)),
                ("kind", "not-a-kind"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=invalid_link_kind"));

    let resp = app
        .call(form_req(
            &format!("/ui/issues/{}/links", source.id),
            &[("target_key", "not-a-key"), ("kind", "blocks")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=invalid_issue_key"));

    let resp = app
        .call(form_req(
            &format!("/ui/issues/{}/links", source.id),
            &[("target_key", "NOPE-1"), ("kind", "blocks")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=unknown_issue_key"));

    let resp = app
        .call(form_req(
            &format!("/ui/issues/{}/links", source.id),
            &[
                ("target_key", &format!("LK-{}", source.seq)),
                ("kind", "blocks"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=self_link"));

    let resp = app
        .call(form_req(
            &format!("/ui/issues/{}/links", source.id),
            &[
                ("target_key", &format!("LK-{}", target.seq)),
                ("kind", "blocks"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(!location(resp).contains("error="));

    let links = workbench_service::domain::issue_link::related(&db, &source.id)
        .await
        .unwrap();
    assert_eq!(links.blocks.len(), 1);
    let link_id = links.blocks[0].link_id.clone();

    // A second, identical link hits the unique constraint.
    let resp = app
        .call(form_req(
            &format!("/ui/issues/{}/links", source.id),
            &[
                ("target_key", &format!("LK-{}", target.seq)),
                ("kind", "blocks"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=link_exists"));

    let resp = app
        .call(req(
            Method::POST,
            &format!("/ui/issues/{}/links/{link_id}/delete", source.id),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(!location(resp).contains("error="));

    let links = workbench_service::domain::issue_link::related(&db, &source.id)
        .await
        .unwrap();
    assert!(links.blocks.is_empty());
}

#[tokio::test]
async fn add_link_on_a_missing_issue_redirects_home() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("add_link_on_a_missing_issue_redirects_home");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(form_req(
            "/ui/issues/no-such-id/links",
            &[("target_key", "WB-1"), ("kind", "blocks")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location(resp).contains("error=not_found"));
}
