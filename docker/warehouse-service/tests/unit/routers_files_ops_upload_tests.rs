use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web;
use quench_db::{Db, InMemoryDb};
use std::path::Path;
use warehouse_service::routers::files::ops::upload::{handle, staging_path};

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

#[actix_web::test]
async fn handle_reports_not_found_when_file_storage_is_disabled() {
    let app = actix_test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(Db::InMemory(InMemoryDb::new())))
            .service(handle),
    )
    .await;
    let req = actix_test::TestRequest::put()
        .uri("/artifacts/file?path=a.txt")
        .set_payload(Vec::<u8>::new())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
