//! HTTP-layer coverage for `routers::api::projects`, `secrets`, `credentials`,
//! and the resource-scoped side of `routers::api::authz::can_on_project` -
//! all need a real Postgres for the tree/queue/store queries underneath them.

use crate::support::{self, TEST_USER, database, json_body, json_req, req, skipped};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::providers::Providers;
use conveyor_service::routers::api;
use http::{Method, StatusCode};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::{Database, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use std::sync::Arc;

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

/// `routers::api::Actor` resolves to the synthetic `"admin"` identity
/// `get_user_from_req` returns whenever `JwtConfig::for_tests()`'s
/// `auth_enabled` is off (the default) and the container actually carries a
/// `JwtConfig` - which it does here, unlike the old actix version's test
/// harness, which never registered one as `app_data` and so fell back to a
/// different, incidental `"dev"` literal instead. `database()` seeds
/// `TEST_USER`, not `"admin"` - so any test that performs a write (stamping
/// `created_by`/`registered_by`, a foreign key into `auth.users`) must seed
/// this too.
async fn seed_admin_user(db: &Db) {
    db.execute("INSERT INTO auth.users (username, password, roles) VALUES ('admin', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING").await.expect("seed the admin user");
}

/// Builds the test app for `db`, after seeding the `"admin"` user `Actor`
/// resolves to, and linking every API router.
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

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_rejects_an_empty_name() {
    let Some((db, _guard)) = database().await else {
        return skipped("create_rejects_an_empty_name");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/projects",
            serde_json::json!({ "name": "   " }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_then_read_round_trips_with_its_full_path() {
    let Some((db, _guard)) = database().await else {
        return skipped("create_then_read_round_trips_with_its_full_path");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/projects",
            serde_json::json!({ "name": "api-project-root" }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created = json_body(resp).await;
    let id = created["id"].as_str().expect("id").to_string();

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/projects/{id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["name"], "api-project-root");
    assert_eq!(body["path"], "api-project-root");
}

#[tokio::test]
async fn read_a_missing_project_is_not_found() {
    let Some((db, _guard)) = database().await else {
        return skipped("read_a_missing_project_is_not_found");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::GET,
            "/api/v1/projects/00000000-0000-0000-0000-000000000000",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_renames_and_rejects_an_empty_name() {
    let Some((db, _guard)) = database().await else {
        return skipped("update_renames_and_rejects_an_empty_name");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/projects",
            serde_json::json!({ "name": "rename-me" }),
            &container,
        ))
        .await;
    let created = json_body(resp).await;
    let id = created["id"].as_str().expect("id");

    let resp = app
        .call(json_req(
            Method::PATCH,
            &format!("/api/v1/projects/{id}"),
            serde_json::json!({ "name": "renamed" }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["name"], "renamed");

    let resp = app
        .call(json_req(
            Method::PATCH,
            &format!("/api/v1/projects/{id}"),
            serde_json::json!({ "name": "  " }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_move_rejects_a_cycle() {
    let Some((db, _guard)) = database().await else {
        return skipped("update_move_rejects_a_cycle");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/projects",
            serde_json::json!({ "name": "cycle-root" }),
            &container,
        ))
        .await;
    let root = json_body(resp).await;
    let root_id = root["id"].as_str().expect("id").to_string();

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/projects",
            serde_json::json!({ "name": "cycle-child", "parent_id": root_id }),
            &container,
        ))
        .await;
    let child = json_body(resp).await;
    let child_id = child["id"].as_str().expect("id");

    let resp = app
        .call(json_req(
            Method::PATCH,
            &format!("/api/v1/projects/{root_id}"),
            serde_json::json!({ "parent_id": child_id }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_conflicts_when_children_exist_then_succeeds_once_they_are_gone() {
    let Some((db, _guard)) = database().await else {
        return skipped("delete_conflicts_when_children_exist_then_succeeds_once_they_are_gone");
    };
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/projects",
            serde_json::json!({ "name": "delete-root" }),
            &container,
        ))
        .await;
    let root = json_body(resp).await;
    let root_id = root["id"].as_str().expect("id").to_string();

    let resp = app
        .call(json_req(
            Method::POST,
            "/api/v1/projects",
            serde_json::json!({ "name": "delete-child", "parent_id": root_id }),
            &container,
        ))
        .await;
    let child = json_body(resp).await;
    let child_id = child["id"].as_str().expect("id").to_string();

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/projects/{root_id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/projects/{child_id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/projects/{root_id}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn list_defaults_to_root_level_projects() {
    let Some((db, _guard)) = database().await else {
        return skipped("list_defaults_to_root_level_projects");
    };
    let (app, container) = app(db).await;

    app.call(json_req(
        Method::POST,
        "/api/v1/projects",
        serde_json::json!({ "name": "list-visible-root" }),
        &container,
    ))
    .await;

    let resp = app
        .call(req(Method::GET, "/api/v1/projects", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
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

#[tokio::test]
async fn estate_secret_round_trips_and_is_removable() {
    let Some((db, _guard)) = database().await else {
        return skipped("estate_secret_round_trips_and_is_removable");
    };
    configure_keys();
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::PUT,
            "/api/v1/secrets/api_token",
            serde_json::json!({ "value": "s3cr3t" }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .call(req(Method::GET, "/api/v1/secrets", &container))
        .await;
    let names = json_body(resp).await;
    let names = names.as_array().expect("array");
    assert!(names.iter().any(|n| n["name"] == "api_token"));

    let resp = app
        .call(req(Method::DELETE, "/api/v1/secrets/api_token", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .call(req(Method::DELETE, "/api/v1/secrets/api_token", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn writing_a_secret_without_the_encryption_key_configured_is_service_unavailable() {
    let Some((db, _guard)) = database().await else {
        return skipped(
            "writing_a_secret_without_the_encryption_key_configured_is_service_unavailable",
        );
    };
    unsafe { std::env::remove_var("CONVEYOR_SECRET_KEY") };
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::PUT,
            "/api/v1/secrets/no_key",
            serde_json::json!({ "value": "x" }),
            &container,
        ))
        .await;
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

#[tokio::test]
async fn repo_secret_round_trips_and_is_scoped_to_that_repo() {
    let Some((db, _guard)) = database().await else {
        return skipped("repo_secret_round_trips_and_is_scoped_to_that_repo");
    };
    configure_keys();
    let (_project_id, repo_id) = make_repo(&db, "repo-secret-project", "repo-secret-repo").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(json_req(
            Method::PUT,
            &format!("/api/v1/repos/{repo_id}/secrets/deploy_key"),
            serde_json::json!({ "value": "deploy-value" }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/repos/{repo_id}/secrets"),
            &container,
        ))
        .await;
    let names = json_body(resp).await;
    let names = names.as_array().expect("array");
    assert!(names.iter().any(|n| n["name"] == "deploy_key"));
}

#[tokio::test]
async fn repo_secret_for_an_unknown_repo_is_not_found() {
    let Some((db, _guard)) = database().await else {
        return skipped("repo_secret_for_an_unknown_repo_is_not_found");
    };
    configure_keys();
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::GET,
            "/api/v1/repos/00000000-0000-0000-0000-000000000000/secrets",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn repo_credential_round_trips_and_shows_null_when_unset() {
    let Some((db, _guard)) = database().await else {
        return skipped("repo_credential_round_trips_and_shows_null_when_unset");
    };
    configure_keys();
    let (_project_id, repo_id) = make_repo(&db, "repo-cred-project", "repo-cred-repo").await;
    let (app, container) = app(db).await;

    let resp = app
        .call(req(
            Method::GET,
            &format!("/api/v1/repos/{repo_id}/credentials"),
            &container,
        ))
        .await;
    let body = json_body(resp).await;
    assert!(body.is_null());

    let resp = app
        .call(json_req(
            Method::PUT,
            &format!("/api/v1/repos/{repo_id}/credentials"),
            serde_json::json!({ "name": "deploy", "username": "git", "token": "ghp_abcdefgh" }),
            &container,
        ))
        .await;
    let status = resp.status();
    let body = support::body_text(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");

    let resp = app
        .call(req(
            Method::DELETE,
            &format!("/api/v1/repos/{repo_id}/credentials"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
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
    let (app, container) = app(db).await;

    let resp = app.call(json_req(Method::PUT, &format!("/api/v1/projects/{}/credentials", project.id), serde_json::json!({ "name": "bad-kind", "kind": "ssh_key", "username": "git", "token": "x" }), &container)).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
