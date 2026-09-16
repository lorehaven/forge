use crate::support;
use http::{Method, StatusCode};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::{Db, InMemoryDb};
use warehouse_service::routers::files;

#[tokio::test]
async fn handle_reports_not_found_when_file_storage_is_disabled() {
    files::register_routes();
    let container = support::container_builder()
        .provide(JwtConfig::for_tests())
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container).await;

    let resp = app
        .call(support::req(
            Method::DELETE,
            "/api/v1/files/artifacts/file?path=a.txt",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
