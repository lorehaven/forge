use actix_web::body::MessageBody;
use actix_web::{App, test as actix_test, web};
use chrono::Utc;
use quench_auth::prelude::JwtConfig;
use quench_db::{Db, InMemoryDb};
use sqlx::types::Json;
use warehouse_service::domain::apk::ApkVersion;
use warehouse_service::routers::ui::pages::apk::catalog::{
    apk_catalog, render_apk_page, unyank_version, yank_version,
};

fn version(package: &str, code: i64, yanked: bool) -> ApkVersion {
    ApkVersion {
        id: ApkVersion::id_for(package, code),
        package_name: package.to_string(),
        version_code: code,
        version_name: format!("{code}.0"),
        min_sdk_version: Some(21),
        target_sdk_version: Some(34),
        label: Some("Test App".to_string()),
        permissions: Json(vec!["android.permission.INTERNET".to_string()]),
        size_bytes: 4096,
        sha256: "deadbeef".to_string(),
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
// render_apk_page
// -----------------------------------------------------------------

#[test]
fn render_apk_page_with_no_versions_shows_the_empty_state() {
    let resp = render_apk_page(&[], None, None, false);
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let html = body_html(resp);
    assert!(html.contains("ui_apk_empty"));
}

#[test]
fn render_apk_page_lists_packages_in_the_tree() {
    let versions = vec![
        version("com.example.one", 1, false),
        version("com.example.two", 5, false),
    ];
    let resp = render_apk_page(&versions, None, None, false);
    let html = body_html(resp);
    assert!(html.contains("com.example.one"));
    assert!(html.contains("com.example.two"));
    assert!(html.contains("ui_apk_empty_select_version"));
}

#[test]
fn render_apk_page_selects_the_newest_version_of_the_chosen_package() {
    let versions = vec![
        version("com.example.app", 1, false),
        version("com.example.app", 3, false),
        version("com.example.app", 2, false),
    ];
    let resp = render_apk_page(&versions, Some("com.example.app"), None, false);
    let html = body_html(resp);
    assert!(html.contains("ui_apk_meta_version_code"));
    // Newest non-selected default is version_code 3.
    assert!(html.contains("3.0"));
}

#[test]
fn render_apk_page_shows_a_yank_button_only_when_the_caller_may_manage() {
    let versions = vec![version("com.example.app", 7, false)];

    let with_manage = body_html(render_apk_page(
        &versions,
        Some("com.example.app"),
        Some(7),
        true,
    ));
    assert!(with_manage.contains("ui_apk_yank"));
    assert!(with_manage.contains("/apk/yank"));

    let without = body_html(render_apk_page(
        &versions,
        Some("com.example.app"),
        Some(7),
        false,
    ));
    assert!(!without.contains("ui_apk_yank"));
    assert!(!without.contains("/apk/yank"));
}

#[test]
fn render_apk_page_offers_unyank_for_a_yanked_version() {
    let versions = vec![version("com.example.app", 9, true)];
    let html = body_html(render_apk_page(
        &versions,
        Some("com.example.app"),
        Some(9),
        true,
    ));
    assert!(html.contains("ui_apk_unyank"));
    assert!(html.contains("ui_status_yanked"));
}

// -----------------------------------------------------------------
// HTTP handlers (the APK feature is off in the test binary, so the
// deterministically reachable branches are login-redirect and disabled)
// -----------------------------------------------------------------

#[actix_web::test]
async fn apk_catalog_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .app_data(in_memory_db())
            .service(apk_catalog),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/apk/catalog")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn apk_catalog_renders_when_authenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .app_data(in_memory_db())
            .service(apk_catalog),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/apk/catalog")
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
        .uri("/apk/yank")
        .set_form([("package", "com.example.app"), ("version_code", "1")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn yank_is_not_found_when_the_apk_feature_is_disabled() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .app_data(in_memory_db())
            .service(yank_version),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/apk/yank")
        .set_form([("package", "com.example.app"), ("version_code", "1")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn unyank_is_not_found_when_the_apk_feature_is_disabled() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .app_data(in_memory_db())
            .service(unyank_version),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/apk/unyank")
        .set_form([("package", "com.example.app"), ("version_code", "1")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}
