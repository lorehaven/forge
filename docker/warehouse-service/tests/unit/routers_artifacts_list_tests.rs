use crate::support;
use http::{Method, StatusCode};
use quench_db::{Db, InMemoryDb};
use warehouse_service::routers::artifacts;

#[tokio::test]
async fn platform_versions_reports_not_found_when_disabled() {
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
            "/api/v1/artifacts/com.example.app/android",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn program_versions_reports_not_found_when_disabled() {
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
            "/api/v1/artifacts/com.example.app",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn catalog_reports_not_found_when_disabled() {
    artifacts::register_routes();
    let container = support::container_builder()
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container).await;

    let resp = app
        .call(support::req(Method::GET, "/api/v1/artifacts", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
