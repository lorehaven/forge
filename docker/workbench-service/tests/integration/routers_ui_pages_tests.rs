//! HTTP-level coverage for `routers::ui`'s pages (home, board, issue detail)
//! against a real Postgres, auth disabled (`JwtConfig::for_tests()` -
//! `is_ui_authenticated`/`actor` both take the bypass documented on
//! `routers::ui::common::actor`, synthesizing an all-access "admin" identity,
//! so every `can_on_project_claims` write check here passes).

use crate::support::{TEST_USER, database};
use actix_web::http::StatusCode;
use actix_web::{App, test as actix_test, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use workbench_service::domain::issue::{self, NewIssue};
use workbench_service::domain::project::{self, NewProject};
use workbench_service::routers::ui;

macro_rules! app {
    ($db:expr) => {
        actix_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtConfig::for_tests()))
                .app_data(web::Data::new($db))
                .service(ui::scope()),
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

fn location_of(resp: &actix_web::dev::ServiceResponse) -> String {
    resp.headers()
        .get("Location")
        .expect("a Location header")
        .to_str()
        .unwrap()
        .to_string()
}

// ---------------------------------------------------------------------------
// Home
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn home_renders_the_empty_state_with_no_projects() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("home_renders_the_empty_state_with_no_projects");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::get().uri("/ui/home").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    assert!(String::from_utf8_lossy(&body).contains("ui_home_no_projects"));
}

#[actix_web::test]
async fn home_slash_lists_a_seeded_project() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("home_slash_lists_a_seeded_project");
    };
    seed_project(&db, "HM").await;
    let app = app!(db);

    let req = actix_test::TestRequest::get().uri("/ui/home/").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    assert!(String::from_utf8_lossy(&body).contains("Project HM"));
}

#[actix_web::test]
async fn home_shows_the_error_notice_from_a_redirect() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("home_shows_the_error_notice_from_a_redirect");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/ui/home?error=key_taken")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    assert!(String::from_utf8_lossy(&body).contains("key_taken"));
}

#[actix_web::test]
async fn create_project_rejects_empty_fields_then_creates_then_rejects_duplicate_key() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "create_project_rejects_empty_fields_then_creates_then_rejects_duplicate_key",
        );
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/ui/projects")
        .set_form([("key", ""), ("name", "")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=empty_fields"));

    let req = actix_test::TestRequest::post()
        .uri("/ui/projects")
        .set_form([("key", "DUP"), ("name", "Dup"), ("description", "d")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("/projects/"));
    assert!(location_of(&resp).contains("/board"));

    let req = actix_test::TestRequest::post()
        .uri("/ui/projects")
        .set_form([("key", "DUP"), ("name", "Dup Again")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=key_taken"));
}

// ---------------------------------------------------------------------------
// Board
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn board_redirects_home_for_a_missing_project() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("board_redirects_home_for_a_missing_project");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/ui/projects/no-such-id/board")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).ends_with("/home"));
}

#[actix_web::test]
async fn board_renders_issues_grouped_by_status() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("board_renders_issues_grouped_by_status");
    };
    let project = seed_project(&db, "BD").await;
    seed_issue(&db, &project.id, "First card").await;
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri(&format!("/ui/projects/{}/board", project.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = String::from_utf8_lossy(&actix_test::read_body(resp).await).into_owned();
    assert!(body.contains("First card"));
    assert!(body.contains("wb-board"));
}

#[actix_web::test]
async fn create_issue_rejects_empty_title_then_creates_and_transitions() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "create_issue_rejects_empty_title_then_creates_and_transitions",
        );
    };
    let project = seed_project(&db, "CI").await;
    let app = app!(db.clone());

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/projects/{}/issues", project.id))
        .set_form([("title", "  ")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=title_required"));

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/projects/{}/issues", project.id))
        .set_form([
            ("title", "New issue"),
            ("kind", "bug"),
            ("priority", "high"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(!location_of(&resp).contains("error="));

    let created = issue::list_by_project(&db, &project.id, None)
        .await
        .unwrap();
    assert_eq!(created.len(), 1);
    let issue_id = created[0].id.clone();

    let req = actix_test::TestRequest::post()
        .uri(&format!(
            "/ui/projects/{}/issues/{issue_id}/transition",
            project.id
        ))
        .set_form([("status", "not-a-status")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let req = actix_test::TestRequest::post()
        .uri(&format!(
            "/ui/projects/{}/issues/{issue_id}/transition",
            project.id
        ))
        .set_form([("status", "done")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = String::from_utf8_lossy(&actix_test::read_body(resp).await).into_owned();
    assert!(body.contains("New issue"));

    let after = issue::read(&db, &issue_id).await.unwrap().unwrap();
    assert_eq!(after.status, "done");
}

#[actix_web::test]
async fn create_issue_reports_an_unknown_assignee_as_a_foreign_key_violation() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "create_issue_reports_an_unknown_assignee_as_a_foreign_key_violation",
        );
    };
    let project = seed_project(&db, "FK").await;
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/projects/{}/issues", project.id))
        .set_form([("title", "Assigned"), ("assignee", "no-such-user")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=unknown_assignee"));
}

// ---------------------------------------------------------------------------
// Issue detail
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn detail_redirects_home_for_a_missing_issue() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("detail_redirects_home_for_a_missing_issue");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/ui/issues/no-such-id")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).ends_with("/home"));
}

#[actix_web::test]
async fn detail_renders_the_issue_with_comments_and_links() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("detail_renders_the_issue_with_comments_and_links");
    };
    let project = seed_project(&db, "DT").await;
    let issue = seed_issue(&db, &project.id, "Detailed issue").await;
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri(&format!("/ui/issues/{}", issue.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = String::from_utf8_lossy(&actix_test::read_body(resp).await).into_owned();
    assert!(body.contains("Detailed issue"));
    assert!(body.contains("ui_issue_comments"));
}

#[actix_web::test]
async fn update_rejects_empty_title_then_applies_a_valid_update() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("update_rejects_empty_title_then_applies_a_valid_update");
    };
    let project = seed_project(&db, "UD").await;
    let issue = seed_issue(&db, &project.id, "Original").await;
    let app = app!(db.clone());

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}", issue.id))
        .set_form([
            ("title", ""),
            ("kind", "task"),
            ("priority", "medium"),
            ("status", "todo"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=title_required"));

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}", issue.id))
        .set_form([
            ("title", "Updated"),
            ("kind", "bug"),
            ("priority", "high"),
            ("status", "in-progress"),
            ("estimate", "5"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(!location_of(&resp).contains("error="));

    let after = issue::read(&db, &issue.id).await.unwrap().unwrap();
    assert_eq!(after.title, "Updated");
    assert_eq!(after.status, "in-progress");
    assert_eq!(after.estimate, Some(5));
}

#[actix_web::test]
async fn update_rejects_a_negative_estimate() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("update_rejects_a_negative_estimate");
    };
    let project = seed_project(&db, "NE").await;
    let issue = seed_issue(&db, &project.id, "Negative").await;
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}", issue.id))
        .set_form([
            ("title", "Negative"),
            ("kind", "task"),
            ("priority", "medium"),
            ("status", "todo"),
            ("estimate", "not-a-number"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=invalid_estimate"));
}

#[actix_web::test]
async fn update_on_a_missing_issue_redirects_home() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("update_on_a_missing_issue_redirects_home");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/ui/issues/no-such-id")
        .set_form([
            ("title", "x"),
            ("kind", "task"),
            ("priority", "medium"),
            ("status", "todo"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=not_found"));
}

#[actix_web::test]
async fn create_comment_rejects_empty_body_then_adds_a_comment() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("create_comment_rejects_empty_body_then_adds_a_comment");
    };
    let project = seed_project(&db, "CC").await;
    let issue = seed_issue(&db, &project.id, "Commented").await;
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}/comments", issue.id))
        .set_form([("body", "   ")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=comment_required"));

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}/comments", issue.id))
        .set_form([("body", "A comment")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(!location_of(&resp).contains("error="));
}

#[actix_web::test]
async fn add_link_validates_kind_key_and_self_link_then_creates_and_removes() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped(
            "add_link_validates_kind_key_and_self_link_then_creates_and_removes",
        );
    };
    let project = seed_project(&db, "LK").await;
    let source = seed_issue(&db, &project.id, "Source").await;
    let target = seed_issue(&db, &project.id, "Target").await;
    let app = app!(db.clone());

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}/links", source.id))
        .set_form(&[
            ("target_key", format!("LK-{}", target.seq)),
            ("kind", "not-a-kind".into()),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=invalid_link_kind"));

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}/links", source.id))
        .set_form(&[
            ("target_key", "not-a-key".to_string()),
            ("kind", "blocks".into()),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=invalid_issue_key"));

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}/links", source.id))
        .set_form(&[
            ("target_key", "NOPE-1".to_string()),
            ("kind", "blocks".into()),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=unknown_issue_key"));

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}/links", source.id))
        .set_form(&[
            ("target_key", format!("LK-{}", source.seq)),
            ("kind", "blocks".into()),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=self_link"));

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}/links", source.id))
        .set_form(&[
            ("target_key", format!("LK-{}", target.seq)),
            ("kind", "blocks".into()),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(!location_of(&resp).contains("error="));

    let related = issue::read(&db, &source.id).await.unwrap().unwrap();
    let _ = related;
    let links = workbench_service::domain::issue_link::related(&db, &source.id)
        .await
        .unwrap();
    assert_eq!(links.blocks.len(), 1);
    let link_id = links.blocks[0].link_id.clone();

    // A second, identical link hits the unique constraint.
    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}/links", source.id))
        .set_form(&[
            ("target_key", format!("LK-{}", target.seq)),
            ("kind", "blocks".into()),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=link_exists"));

    let req = actix_test::TestRequest::post()
        .uri(&format!("/ui/issues/{}/links/{link_id}/delete", source.id))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(!location_of(&resp).contains("error="));

    let links = workbench_service::domain::issue_link::related(&db, &source.id)
        .await
        .unwrap();
    assert!(links.blocks.is_empty());
}

#[actix_web::test]
async fn add_link_on_a_missing_issue_redirects_home() {
    let Some((db, _guard)) = database().await else {
        return crate::support::skipped("add_link_on_a_missing_issue_redirects_home");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/ui/issues/no-such-id/links")
        .set_form([("target_key", "WB-1"), ("kind", "blocks")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert!(location_of(&resp).contains("error=not_found"));
}
