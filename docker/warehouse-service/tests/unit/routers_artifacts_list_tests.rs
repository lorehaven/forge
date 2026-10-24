use crate::support;
use quench_db::{Db, InMemoryDb};
use quench_http::http::{Method, StatusCode};
use warehouse_service::routers::artifacts;

#[quench_test::test]
async fn platform_versions_reports_not_found_when_disabled() {
    artifacts::register_routes();
    let container = support::container_builder()
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container);

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/artifacts/com.example.app/android",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn program_versions_reports_not_found_when_disabled() {
    artifacts::register_routes();
    let container = support::container_builder()
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container);

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/artifacts/com.example.app",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn catalog_reports_not_found_when_disabled() {
    artifacts::register_routes();
    let container = support::container_builder()
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container);

    let resp = app
        .call(support::req(Method::GET, "/api/v1/artifacts", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
