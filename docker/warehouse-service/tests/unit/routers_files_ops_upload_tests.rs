use crate::support;
use http::{Method, StatusCode};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::{Db, InMemoryDb};
use std::path::Path;
use warehouse_service::routers::files;
use warehouse_service::routers::files::ops::upload::staging_path;

#[test]
fn staging_path_sits_beside_the_target_with_a_dotted_part_suffix() {
    let target = Path::new("/storage/artifacts/report.pdf");
    let staging = staging_path(target);
    assert_eq!(staging.parent(), Some(Path::new("/storage/artifacts")));
    let name = staging.file_name().unwrap().to_str().unwrap();
    assert!(name.starts_with(".report.pdf."), "{name}");
    assert!(name.ends_with(".part"), "{name}");
}

#[test]
fn staging_path_calls_never_collide_even_for_the_same_target() {
    let target = Path::new("/storage/artifacts/report.pdf");
    let a = staging_path(target);
    let b = staging_path(target);
    assert_ne!(a, b);
}

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
        .call(support::raw_req(
            Method::PUT,
            "/api/v1/files/artifacts/file?path=a.txt",
            &[],
            &[],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
