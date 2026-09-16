use crate::support;
use http::{Method, StatusCode};
use quench_db::{Db, InMemoryDb};
use warehouse_service::routers::artifacts;

#[tokio::test]
async fn handle_reports_not_found_when_artifact_storage_is_disabled() {
    // `FEATURE_ARTIFACTS_ENABLED` / `FEATURE_APK_ENABLED` are unset in this
    // sandbox, and the flag is a `LazyLock` fixed for the whole test binary
    // (see `routers_files_ops_download_tests`'s own comment for the same
    // reasoning) - so this deterministically hits the "not enabled" branch
    // rather than ever touching the filesystem or the database.
    artifacts::register_routes();
    let container = support::container_builder()
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container).await;

    let resp = app
        .call(support::raw_req(
            Method::PUT,
            "/api/v1/artifacts/com.example.app/android/1",
            &[],
            &[],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
