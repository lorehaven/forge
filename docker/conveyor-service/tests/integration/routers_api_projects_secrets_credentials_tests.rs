//! HTTP-layer coverage for `routers::api::projects`, `secrets`, `credentials`,
//! and the resource-scoped side of `routers::api::authz::can_on_project` -
//! all need a real Postgres for the tree/queue/store queries underneath them.

use crate::support::{TEST_USER, database, skipped};
use actix_web::http::StatusCode;
use actix_web::{App, test as actix_test, web};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::providers::Providers;
use conveyor_service::routers::api;
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::{Database, Db};

const SECRET_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const CREDENTIAL_KEY: &str = "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";

/// Set under the database guard (like `secrets_store_tests.rs` does), so
/// these do not race any other integration test in this crate over the same
/// env vars.
fn configure_keys() {
    unsafe {
        std::env::set_var("CONVEYOR_SECRET_KEY", SECRET_KEY);
        std::env::set_var("CONVEYOR_CREDENTIAL_KEY", CREDENTIAL_KEY);
    }
}

/// `routers::api::actor` falls back to the literal user `"dev"` when a
/// request carries no resolvable identity (no `Auth`-verified claims and no
/// session cookie - true of every request built by these tests, which drive
/// the API directly rather than going through a login), and every write
/// route stamps that name into `created_by`/`registered_by`, which is a
/// foreign key into `auth.users`. `database()` seeds `TEST_USER`, not
/// `"dev"` - so any test that performs a write must seed this too.
async fn seed_dev_user(db: &Db) {
    db.execute(
        "INSERT INTO auth.users (username, password, roles) \
         VALUES ('dev', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING",
    )
    .await
    .expect("seed the dev user");
}

/// Builds the test app for `db`, after seeding the `"dev"` user `actor()`
/// falls back to. A macro rather than an `async fn` so the caller doesn't
/// need to name `actix_test::init_service`'s opaque service type.
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

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn create_rejects_an_empty_name() {
    let Some((db, _guard)) = database().await else {
        return skipped("create_rejects_an_empty_name");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/projects")
        .set_json(serde_json::json!({ "name": "   " }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn create_then_read_round_trips_with_its_full_path() {
    let Some((db, _guard)) = database().await else {
        return skipped("create_then_read_round_trips_with_its_full_path");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/projects")
        .set_json(serde_json::json!({ "name": "api-project-root" }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created: serde_json::Value = actix_test::read_body_json(resp).await;
    let id = created["id"].as_str().expect("id").to_string();

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/projects/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["name"], "api-project-root");
    assert_eq!(body["path"], "api-project-root");
}

#[actix_web::test]
async fn read_a_missing_project_is_not_found() {
    let Some((db, _guard)) = database().await else {
        return skipped("read_a_missing_project_is_not_found");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/projects/00000000-0000-0000-0000-000000000000")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn update_renames_and_rejects_an_empty_name() {
    let Some((db, _guard)) = database().await else {
        return skipped("update_renames_and_rejects_an_empty_name");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/projects")
        .set_json(serde_json::json!({ "name": "rename-me" }))
        .to_request();
    let created: serde_json::Value =
        actix_test::read_body_json(actix_test::call_service(&app, req).await).await;
    let id = created["id"].as_str().expect("id");

    let req = actix_test::TestRequest::patch()
        .uri(&format!("/api/v1/projects/{id}"))
        .set_json(serde_json::json!({ "name": "renamed" }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["name"], "renamed");

    let req = actix_test::TestRequest::patch()
        .uri(&format!("/api/v1/projects/{id}"))
        .set_json(serde_json::json!({ "name": "  " }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn update_move_rejects_a_cycle() {
    let Some((db, _guard)) = database().await else {
        return skipped("update_move_rejects_a_cycle");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/projects")
        .set_json(serde_json::json!({ "name": "cycle-root" }))
        .to_request();
    let root: serde_json::Value =
        actix_test::read_body_json(actix_test::call_service(&app, req).await).await;
    let root_id = root["id"].as_str().expect("id").to_string();

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/projects")
        .set_json(serde_json::json!({ "name": "cycle-child", "parent_id": root_id }))
        .to_request();
    let child: serde_json::Value =
        actix_test::read_body_json(actix_test::call_service(&app, req).await).await;
    let child_id = child["id"].as_str().expect("id");

    let req = actix_test::TestRequest::patch()
        .uri(&format!("/api/v1/projects/{root_id}"))
        .set_json(serde_json::json!({ "parent_id": child_id }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn delete_conflicts_when_children_exist_then_succeeds_once_they_are_gone() {
    let Some((db, _guard)) = database().await else {
        return skipped("delete_conflicts_when_children_exist_then_succeeds_once_they_are_gone");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/projects")
        .set_json(serde_json::json!({ "name": "delete-root" }))
        .to_request();
    let root: serde_json::Value =
        actix_test::read_body_json(actix_test::call_service(&app, req).await).await;
    let root_id = root["id"].as_str().expect("id").to_string();

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/projects")
        .set_json(serde_json::json!({ "name": "delete-child", "parent_id": root_id }))
        .to_request();
    let child: serde_json::Value =
        actix_test::read_body_json(actix_test::call_service(&app, req).await).await;
    let child_id = child["id"].as_str().expect("id").to_string();

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/projects/{root_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/projects/{child_id}"))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/projects/{root_id}"))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );
}

#[actix_web::test]
async fn list_defaults_to_root_level_projects() {
    let Some((db, _guard)) = database().await else {
        return skipped("list_defaults_to_root_level_projects");
    };
    let app = app!(db);

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/projects")
        .set_json(serde_json::json!({ "name": "list-visible-root" }))
        .to_request();
    actix_test::call_service(&app, req).await;

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/projects")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let names: Vec<&str> = body
        .as_array()
        .expect("array")
        .iter()
        .map(|p| p["name"].as_str().expect("name"))
        .collect();
    assert!(names.contains(&"list-visible-root"));
}

// ---------------------------------------------------------------------------
// Secrets (estate-wide)
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn estate_secret_round_trips_and_is_removable() {
    let Some((db, _guard)) = database().await else {
        return skipped("estate_secret_round_trips_and_is_removable");
    };
    configure_keys();
    let app = app!(db);

    let req = actix_test::TestRequest::put()
        .uri("/api/v1/secrets/api_token")
        .set_json(serde_json::json!({ "value": "s3cr3t" }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/secrets")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    let names: Vec<serde_json::Value> = actix_test::read_body_json(resp).await;
    assert!(names.iter().any(|n| n["name"] == "api_token"));

    let req = actix_test::TestRequest::delete()
        .uri("/api/v1/secrets/api_token")
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );

    let req = actix_test::TestRequest::delete()
        .uri("/api/v1/secrets/api_token")
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, req).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[actix_web::test]
async fn writing_a_secret_without_the_encryption_key_configured_is_service_unavailable() {
    let Some((db, _guard)) = database().await else {
        return skipped(
            "writing_a_secret_without_the_encryption_key_configured_is_service_unavailable",
        );
    };
    unsafe { std::env::remove_var("CONVEYOR_SECRET_KEY") };
    let app = app!(db);

    let req = actix_test::TestRequest::put()
        .uri("/api/v1/secrets/no_key")
        .set_json(serde_json::json!({ "value": "x" }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Restore for tests that run after this one under the same guard window.
    configure_keys();
}

// ---------------------------------------------------------------------------
// Repo-scoped secrets and credentials, and the shared repo_scope() helper
// ---------------------------------------------------------------------------

async fn make_repo(db: &Db, project_name: &str, repo_name: &str) -> (String, String) {
    let project = conveyor_service::scheduler::projects::create(
        db,
        &conveyor_service::scheduler::projects::NewProject {
            name: project_name.to_string(),
            parent_id: None,
        },
    )
    .await
    .expect("create project");

    let repo = conveyor_service::scheduler::repos::create(
        db,
        &conveyor_service::scheduler::repos::NewRepo {
            provider: conveyor_service::domain::Provider::Generic,
            owner: "tests".to_string(),
            name: repo_name.to_string(),
            clone_url: format!("file:///tmp/{repo_name}"),
            default_branch: "master".to_string(),
            registered_by: TEST_USER.to_string(),
            project_id: project.id.clone(),
        },
    )
    .await
    .expect("create repo");

    (project.id, repo.id)
}

#[actix_web::test]
async fn repo_secret_round_trips_and_is_scoped_to_that_repo() {
    let Some((db, _guard)) = database().await else {
        return skipped("repo_secret_round_trips_and_is_scoped_to_that_repo");
    };
    configure_keys();
    let (_project_id, repo_id) = make_repo(&db, "repo-secret-project", "repo-secret-repo").await;
    let app = app!(db);

    let req = actix_test::TestRequest::put()
        .uri(&format!("/api/v1/repos/{repo_id}/secrets/deploy_key"))
        .set_json(serde_json::json!({ "value": "deploy-value" }))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, req).await.status(),
        StatusCode::OK
    );

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/repos/{repo_id}/secrets"))
        .to_request();
    let names: Vec<serde_json::Value> =
        actix_test::read_body_json(actix_test::call_service(&app, req).await).await;
    assert!(names.iter().any(|n| n["name"] == "deploy_key"));
}

#[actix_web::test]
async fn repo_secret_for_an_unknown_repo_is_not_found() {
    let Some((db, _guard)) = database().await else {
        return skipped("repo_secret_for_an_unknown_repo_is_not_found");
    };
    configure_keys();
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri("/api/v1/repos/00000000-0000-0000-0000-000000000000/secrets")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn repo_credential_round_trips_and_shows_null_when_unset() {
    let Some((db, _guard)) = database().await else {
        return skipped("repo_credential_round_trips_and_shows_null_when_unset");
    };
    configure_keys();
    let (_project_id, repo_id) = make_repo(&db, "repo-cred-project", "repo-cred-repo").await;
    let app = app!(db);

    let req = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/repos/{repo_id}/credentials"))
        .to_request();
    let body: serde_json::Value =
        actix_test::read_body_json(actix_test::call_service(&app, req).await).await;
    assert!(body.is_null());

    let req = actix_test::TestRequest::put()
        .uri(&format!("/api/v1/repos/{repo_id}/credentials"))
        .set_json(serde_json::json!({
            "name": "deploy",
            "username": "git",
            "token": "ghp_abcdefgh",
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    let status = resp.status();
    let body = actix_test::read_body(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "body: {}",
        String::from_utf8_lossy(&body)
    );

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/api/v1/repos/{repo_id}/credentials"))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, req).await.status(),
        StatusCode::NO_CONTENT
    );
}

#[actix_web::test]
async fn credential_rejects_an_unsupported_kind() {
    let Some((db, _guard)) = database().await else {
        return skipped("credential_rejects_an_unsupported_kind");
    };
    configure_keys();
    let project = conveyor_service::scheduler::projects::create(
        &db,
        &conveyor_service::scheduler::projects::NewProject {
            name: "cred-kind-project".to_string(),
            parent_id: None,
        },
    )
    .await
    .expect("create project");
    let app = app!(db);

    let req = actix_test::TestRequest::put()
        .uri(&format!("/api/v1/projects/{}/credentials", project.id))
        .set_json(serde_json::json!({
            "name": "bad-kind",
            "kind": "ssh_key",
            "username": "git",
            "token": "x",
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
