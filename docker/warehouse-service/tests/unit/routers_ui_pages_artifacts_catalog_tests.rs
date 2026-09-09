use actix_web::body::MessageBody;
use actix_web::{App, test as actix_test, web};
use chrono::Utc;
use quench_auth::prelude::JwtConfig;
use quench_db::{Db, InMemoryDb};
use sqlx::types::Json;
use warehouse_service::domain::artifact::{ArtifactMetadata, ArtifactVersion, Platform};
use warehouse_service::routers::ui::pages::artifacts::catalog::{
    artifacts_catalog, render_artifacts_page, unyank_version, yank_version,
};

fn version(program: &str, platform: Platform, code: i64, yanked: bool) -> ArtifactVersion {
    ArtifactVersion {
        id: ArtifactVersion::id_for(program, platform, code),
        program: program.to_string(),
        platform: platform.as_str().to_string(),
        arch: None,
        format: if platform == Platform::Android {
            "apk"
        } else {
            "tar.gz"
        }
        .to_string(),
        version_code: code,
        version_name: format!("{code}.0"),
        filename: format!("{program}-{code}"),
        size_bytes: 4096,
        sha256: "deadbeef".to_string(),
        label: Some("Test App".to_string()),
        metadata: Json(ArtifactMetadata::default()),
        uploaded_by: "dev".to_string(),
        yanked,
        created_at: Utc::now(),
    }
}

fn body_html(resp: actix_web::HttpResponse) -> String {
    let body = resp.into_body().try_into_bytes().unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

fn jwt_config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

fn in_memory_db() -> web::Data<Db> {
    web::Data::new(Db::InMemory(InMemoryDb::new()))
}

// -----------------------------------------------------------------
// render_artifacts_page
// -----------------------------------------------------------------

#[test]
fn render_with_no_versions_shows_the_empty_state() {
    let resp = render_artifacts_page(&[], None, None, None, false);
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert!(body_html(resp).contains("ui_artifact_empty"));
}

#[test]
fn render_lists_programs_in_the_tree() {
    let versions = vec![
        version("com.example.one", Platform::Android, 1, false),
        version("com.example.two", Platform::Linux, 5, false),
    ];
    let html = body_html(render_artifacts_page(&versions, None, None, None, false));
    assert!(html.contains("com.example.one"));
    assert!(html.contains("com.example.two"));
    assert!(html.contains("ui_artifact_empty_select_version"));
}

#[test]
fn render_selects_the_newest_version_of_the_chosen_program_platform() {
    let versions = vec![
        version("com.example.app", Platform::Android, 1, false),
        version("com.example.app", Platform::Android, 3, false),
        version("com.example.app", Platform::Android, 2, false),
    ];
    let html = body_html(render_artifacts_page(
        &versions,
        Some("com.example.app"),
        Some("android"),
        None,
        false,
    ));
    assert!(html.contains("ui_artifact_meta_version_code"));
    assert!(html.contains("3.0"));
}

#[test]
fn render_shows_a_yank_button_only_when_the_caller_may_manage() {
    let versions = vec![version("com.example.app", Platform::Android, 7, false)];

    let with_manage = body_html(render_artifacts_page(
        &versions,
        Some("com.example.app"),
        Some("android"),
        Some(7),
        true,
    ));
    assert!(with_manage.contains("ui_artifact_yank"));
    assert!(with_manage.contains("/artifacts/yank"));

    let without = body_html(render_artifacts_page(
        &versions,
        Some("com.example.app"),
        Some("android"),
        Some(7),
        false,
    ));
    assert!(!without.contains("ui_artifact_yank"));
    assert!(!without.contains("/artifacts/yank"));
}

#[test]
fn render_offers_unyank_for_a_yanked_version() {
    let versions = vec![version("com.example.app", Platform::Linux, 9, true)];
    let html = body_html(render_artifacts_page(
        &versions,
        Some("com.example.app"),
        Some("linux"),
        Some(9),
        true,
    ));
    assert!(html.contains("ui_artifact_unyank"));
    assert!(html.contains("ui_status_yanked"));
}

// -----------------------------------------------------------------
// HTTP handlers (the artifact feature is off in the test binary, so the
// deterministically reachable branches are login-redirect and disabled)
// -----------------------------------------------------------------

#[actix_web::test]
async fn catalog_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .app_data(in_memory_db())
            .service(artifacts_catalog),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/artifacts/catalog")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn catalog_renders_when_authenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .app_data(in_memory_db())
            .service(artifacts_catalog),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/artifacts/catalog")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn yank_redirects_to_login_without_a_session() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .app_data(in_memory_db())
            .service(yank_version),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/artifacts/yank")
        .set_form([
            ("program", "com.example.app"),
            ("platform", "android"),
            ("version_code", "1"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn yank_is_not_found_when_the_feature_is_disabled() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .app_data(in_memory_db())
            .service(yank_version),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/artifacts/yank")
        .set_form([
            ("program", "com.example.app"),
            ("platform", "android"),
            ("version_code", "1"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn unyank_is_not_found_when_the_feature_is_disabled() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .app_data(in_memory_db())
            .service(unyank_version),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/artifacts/unyank")
        .set_form([
            ("program", "com.example.app"),
            ("platform", "android"),
            ("version_code", "1"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}
