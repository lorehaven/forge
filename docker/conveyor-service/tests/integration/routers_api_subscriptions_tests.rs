//! `/api/v1/.../subscription` and `/api/v1/subscriptions` - following a repository or project.

use crate::support::{database, json_body, req, skipped};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::providers::Providers;
use conveyor_service::routers::api;
use http::{Method, StatusCode};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::{Database, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use std::sync::Arc;

/// With auth off `Actor` is the synthetic `admin`; following writes a foreign key to the realm's
/// users, so it has to exist (see `routers_api_repos_runs_tests`).
async fn app(db: Db, with_admin: bool) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    if with_admin {
        db.execute("INSERT INTO auth.users (username, password, roles) VALUES ('admin', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING")
            .await
            .expect("seed the admin user");
    }
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

#[tokio::test]
async fn following_a_repo_then_listing_then_leaving() {
    let Some((db, _guard)) = database().await else {
        return skipped("following_a_repo_then_listing_then_leaving");
    };
    let repo = crate::support::register_repo(&db, "follow", "file:///nowhere").await;
    let (app, container) = app(db, true).await;
    let path = format!("/api/v1/repos/{}/subscription", repo.id);

    let first = app.call(req(Method::PUT, &path, &container)).await;
    assert_eq!(first.status(), StatusCode::CREATED);
    assert_eq!(json_body(first).await["subscribed"], true);
    // Following again is fine and says so.
    let again = app.call(req(Method::PUT, &path, &container)).await;
    assert_eq!(again.status(), StatusCode::OK);

    let listed = app
        .call(req(Method::GET, "/api/v1/subscriptions", &container))
        .await;
    assert_eq!(listed.status(), StatusCode::OK);
    let body = json_body(listed).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["repo_id"], repo.id.as_str());
    assert!(body[0]["project_id"].is_null());

    let left = app.call(req(Method::DELETE, &path, &container)).await;
    assert_eq!(left.status(), StatusCode::OK);
    assert_eq!(json_body(left).await["subscribed"], false);
    let listed = app
        .call(req(Method::GET, "/api/v1/subscriptions", &container))
        .await;
    assert!(json_body(listed).await.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn following_a_project_covers_it_by_project_id() {
    let Some((db, _guard)) = database().await else {
        return skipped("following_a_project_covers_it_by_project_id");
    };
    let repo = crate::support::register_repo(&db, "tree", "file:///nowhere").await;
    let (app, container) = app(db, true).await;
    let path = format!("/api/v1/projects/{}/subscription", repo.project_id);

    let followed = app.call(req(Method::PUT, &path, &container)).await;
    assert_eq!(followed.status(), StatusCode::CREATED);
    let listed = app
        .call(req(Method::GET, "/api/v1/subscriptions", &container))
        .await;
    let body = json_body(listed).await;
    assert_eq!(body[0]["project_id"], repo.project_id.as_str());
    assert!(body[0]["repo_id"].is_null());

    let left = app.call(req(Method::DELETE, &path, &container)).await;
    assert_eq!(left.status(), StatusCode::OK);
}

#[tokio::test]
async fn following_something_that_does_not_exist_is_not_found() {
    let Some((db, _guard)) = database().await else {
        return skipped("following_something_that_does_not_exist_is_not_found");
    };
    let (app, container) = app(db, true).await;

    for path in [
        "/api/v1/repos/nope/subscription",
        "/api/v1/projects/nope/subscription",
    ] {
        let resp = app.call(req(Method::PUT, path, &container)).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn someone_who_is_not_in_the_realm_is_told_so_rather_than_getting_a_500() {
    let Some((db, _guard)) = database().await else {
        return skipped("someone_who_is_not_in_the_realm_is_told_so_rather_than_getting_a_500");
    };
    let repo = crate::support::register_repo(&db, "stranger", "file:///nowhere").await;
    // No `admin` row: the request comes from an account the realm does not know.
    db.execute("DELETE FROM auth.users WHERE username = 'admin'")
        .await
        .unwrap();
    let (app, container) = app(db, false).await;

    let resp = app
        .call(req(
            Method::PUT,
            &format!("/api/v1/repos/{}/subscription", repo.id),
            &container,
        ))
        .await;
    let status = resp.status();
    // Other tests share the realm's `admin` row and expect to find it.
    let db = quench_db::prelude::Db::connect(&std::env::var("CONVEYOR_TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    db.execute("INSERT INTO auth.users (username, password, roles) VALUES ('admin', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING")
        .await
        .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
