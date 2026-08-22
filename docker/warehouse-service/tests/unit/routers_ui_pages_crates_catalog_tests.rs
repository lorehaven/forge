use crate::support;

use actix_web::body::MessageBody;
use actix_web::{App, test as actix_test, web};
use quench_auth::prelude::JwtConfig;
use std::collections::HashMap;
use support::WithCratesStorageRoot as WithStorageRoot;
use warehouse_service::routers::crates::index_file_path;
use warehouse_service::routers::ui::pages::crates::catalog::{
    crates_index, crates_index_slash, render_crates_page, unyank_version, yank_version,
};
use warehouse_service::routers::ui::pages::crates::storage::{IndexDep, IndexRecord};

fn write_index_line(name: &str, record: &IndexRecord) {
    let path = index_file_path(name).expect("valid name");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut content = std::fs::read_to_string(&path).unwrap_or_default();
    content.push_str(&serde_json::to_string(record).unwrap());
    content.push('\n');
    std::fs::write(&path, content).unwrap();
}

fn sample_record(version: &str, yanked: bool) -> IndexRecord {
    IndexRecord {
        name: "my-crate".to_string(),
        vers: version.to_string(),
        deps: vec![],
        cksum: "abc123".to_string(),
        features: HashMap::new(),
        features2: None,
        yanked,
        links: None,
        rust_version: None,
        v: 1,
    }
}

fn body_html(resp: actix_web::HttpResponse) -> String {
    let body = resp.into_body().try_into_bytes().unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

// -----------------------------------------------------------------
// render_crates_page
// -----------------------------------------------------------------

#[test]
fn render_crates_page_with_no_crates_renders_ok() {
    let _storage = WithStorageRoot::new();
    let resp = render_crates_page(None, None);
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let html = body_html(resp);
    assert!(html.contains("ui_crates_empty"));
}

#[test]
fn render_crates_page_with_an_unknown_selected_crate_ignores_the_selection() {
    let storage = WithStorageRoot::new();
    write_index_line("my-crate", &sample_record("1.0.0", false));
    let _ = &storage;

    let resp = render_crates_page(Some("no-such-crate".to_string()), None);
    let html = body_html(resp);
    assert!(html.contains("ui_empty_select_version"));
}

#[test]
fn render_crates_page_defaults_to_the_latest_non_yanked_version() {
    let storage = WithStorageRoot::new();
    write_index_line("my-crate", &sample_record("1.0.0", false));
    write_index_line("my-crate", &sample_record("2.0.0", true));
    let _ = &storage;

    let resp = render_crates_page(Some("my-crate".to_string()), None);
    let html = body_html(resp);
    assert!(html.contains("1.0.0"));
    assert!(html.contains("ui_yank_version"));
}

#[test]
fn render_crates_page_falls_back_to_the_last_version_when_all_are_yanked() {
    let storage = WithStorageRoot::new();
    write_index_line("my-crate", &sample_record("1.0.0", true));
    let _ = &storage;

    let resp = render_crates_page(Some("my-crate".to_string()), None);
    let html = body_html(resp);
    assert!(html.contains("ui_status_yanked"));
    assert!(html.contains("ui_unyank_version"));
}

#[test]
fn render_crates_page_shows_deps_features_and_metadata_fields() {
    let storage = WithStorageRoot::new();
    let mut record = sample_record("1.0.0", false);
    record.rust_version = Some("1.75".to_string());
    record.links = Some("libfoo".to_string());
    let mut features = HashMap::new();
    features.insert("default".to_string(), vec![]);
    record.features = features;
    record.deps = vec![
        IndexDep {
            name: "normal-dep".to_string(),
            req: "^1".to_string(),
            features: vec![],
            optional: false,
            default_features: true,
            target: None,
            kind: "normal".to_string(),
            registry: None,
            package: None,
        },
        IndexDep {
            name: "dev-dep".to_string(),
            req: "^1".to_string(),
            features: vec![],
            optional: true,
            default_features: true,
            target: Some("cfg(unix)".to_string()),
            kind: "dev".to_string(),
            registry: None,
            package: None,
        },
        IndexDep {
            name: "build-dep".to_string(),
            req: "^1".to_string(),
            features: vec![],
            optional: false,
            default_features: true,
            target: None,
            kind: "build".to_string(),
            registry: None,
            package: None,
        },
    ];
    write_index_line("my-crate", &record);
    let _ = &storage;

    let resp = render_crates_page(Some("my-crate".to_string()), Some("1.0.0".to_string()));
    let html = body_html(resp);
    assert!(html.contains("1.75"));
    assert!(html.contains("libfoo"));
    assert!(html.contains("default"));
    assert!(html.contains("normal-dep"));
    assert!(html.contains("dev-dep"));
    assert!(html.contains("build-dep"));
    assert!(html.contains("[optional]"));
    assert!(html.contains("[target: cfg(unix)]"));
}

// -----------------------------------------------------------------
// HTTP handlers
// -----------------------------------------------------------------

fn jwt_config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

#[actix_web::test]
async fn crates_index_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .service(crates_index),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/crates/catalog")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn crates_index_slash_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .service(crates_index_slash),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/crates/catalog/")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn crates_index_renders_the_page_when_authenticated() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .service(crates_index),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/crates/catalog")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn yank_version_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .service(yank_version),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/crates/yank")
        .set_form([("name", "my-crate"), ("version", "1.0.0")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn yank_version_reports_not_found_for_an_unknown_version_when_authenticated() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .service(yank_version),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/crates/yank")
        .set_form([("name", "no-such-crate"), ("version", "1.0.0")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn yank_version_yanks_an_existing_version_when_authenticated() {
    let storage = WithStorageRoot::new();
    write_index_line("my-crate", &sample_record("1.0.0", false));
    let _ = &storage;

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .service(yank_version),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/crates/yank")
        .set_form([("name", "my-crate"), ("version", "1.0.0")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);
    assert!(resp.headers().contains_key("HX-Redirect"));
}

#[actix_web::test]
async fn unyank_version_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .service(unyank_version),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/crates/unyank")
        .set_form([("name", "my-crate"), ("version", "1.0.0")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn unyank_version_unyanks_an_existing_version_when_authenticated() {
    let storage = WithStorageRoot::new();
    write_index_line("my-crate", &sample_record("1.0.0", true));
    let _ = &storage;

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .service(unyank_version),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/crates/unyank")
        .set_form([("name", "my-crate"), ("version", "1.0.0")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);
}
