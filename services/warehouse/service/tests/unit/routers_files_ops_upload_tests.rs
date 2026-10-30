use crate::support;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::{Db, InMemoryDb};
use quench_http::http::{Method, StatusCode};
use std::path::Path;
use warehouse_service::routers::files;
use warehouse_service::routers::files::ops::upload::staging_path;

#[quench_test::test]
fn staging_path_sits_beside_the_target_with_a_dotted_part_suffix() {
    let target = Path::new("/storage/artifacts/report.pdf");
    let staging = staging_path(target);
    assert_eq!(staging.parent(), Some(Path::new("/storage/artifacts")));
    let name = staging.file_name().unwrap().to_str().unwrap();
    assert!(name.starts_with(".report.pdf."), "{name}");
    assert!(
        Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("part")),
        "{name}"
    );
}

#[quench_test::test]
fn staging_path_calls_never_collide_even_for_the_same_target() {
    let target = Path::new("/storage/artifacts/report.pdf");
    let a = staging_path(target);
    let b = staging_path(target);
    assert_ne!(a, b);
}

#[quench_test::test]
async fn handle_reports_not_found_when_file_storage_is_disabled() {
    files::register_routes();
    let container = support::container_builder()
        .provide(JwtConfig::for_tests())
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container);

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
