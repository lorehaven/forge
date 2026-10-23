use crate::support;
use quench_db::{Db, InMemoryDb};
use quench_http::http::{Method, StatusCode};
use warehouse_service::routers::artifacts;

#[tokio::test]
async fn handle_reports_not_found_when_artifact_storage_is_disabled() {
    artifacts::register_routes();
    let container = support::container_builder()
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container);

    let resp = app
        .call(support::req(
            Method::DELETE,
            "/api/v1/artifacts/com.example.app/android/1/yank",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
