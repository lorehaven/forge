//! `latest`/`latest/download` used to be their own routes
//! (`routers::artifacts::ops::latest::metadata`/`download`); they're now a
//! sentinel value inside `ops::metadata::handle`/`ops::download::handle`,
//! because quench-http's router can't safely disambiguate two same-shape
//! overlapping patterns (`.../latest` vs `.../{version_code}`) by
//! registration order the way actix could - see `ops::latest`'s doc
//! comment. From an HTTP client's perspective nothing changed: `latest` is
//! still a valid path segment on the same routes.

use crate::support;
use http::{Method, StatusCode};
use quench_db::{Db, InMemoryDb};
use warehouse_service::routers::artifacts;

#[tokio::test]
async fn metadata_reports_not_found_when_artifact_storage_is_disabled() {
    artifacts::register_routes();
    let container = support::container_builder()
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container).await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/artifacts/com.example.app/android/latest",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn download_reports_not_found_when_artifact_storage_is_disabled() {
    artifacts::register_routes();
    let container = support::container_builder()
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container).await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/artifacts/com.example.app/android/latest/download",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
