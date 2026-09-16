use crate::support;

use http::{Method, StatusCode};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::endpoint::Endpoint;
use std::collections::HashMap;
use support::WithCratesStorageRoot as WithStorageRoot;
use warehouse_service::routers::crates::index_file_path;
use warehouse_service::routers::ui::pages::crates::catalog::render_crates_page;
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

async fn body_html(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.unwrap();
    String::from_utf8(collected.to_bytes().to_vec()).unwrap()
}

// -----------------------------------------------------------------
// render_crates_page
// -----------------------------------------------------------------

#[tokio::test]
async fn render_crates_page_with_no_crates_renders_ok() {
    let _storage = WithStorageRoot::new();
    let resp = render_crates_page(None, None);
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_html(resp).await;
    assert!(html.contains("ui_crates_empty"));
}

#[tokio::test]
async fn render_crates_page_with_an_unknown_selected_crate_ignores_the_selection() {
    let storage = WithStorageRoot::new();
    write_index_line("my-crate", &sample_record("1.0.0", false));
    let _ = &storage;

    let resp = render_crates_page(Some("no-such-crate".to_string()), None);
    let html = body_html(resp).await;
    assert!(html.contains("ui_empty_select_version"));
}

#[tokio::test]
async fn render_crates_page_defaults_to_the_latest_non_yanked_version() {
    let storage = WithStorageRoot::new();
    write_index_line("my-crate", &sample_record("1.0.0", false));
    write_index_line("my-crate", &sample_record("2.0.0", true));
    let _ = &storage;

    let resp = render_crates_page(Some("my-crate".to_string()), None);
    let html = body_html(resp).await;
    assert!(html.contains("1.0.0"));
    assert!(html.contains("ui_yank_version"));
}

#[tokio::test]
async fn render_crates_page_falls_back_to_the_last_version_when_all_are_yanked() {
    let storage = WithStorageRoot::new();
    write_index_line("my-crate", &sample_record("1.0.0", true));
    let _ = &storage;

    let resp = render_crates_page(Some("my-crate".to_string()), None);
    let html = body_html(resp).await;
    assert!(html.contains("ui_status_yanked"));
    assert!(html.contains("ui_unyank_version"));
}

#[tokio::test]
async fn render_crates_page_shows_deps_features_and_metadata_fields() {
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
    let html = body_html(resp).await;
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

async fn app(
    jwt_config: JwtConfig,
) -> (
    std::sync::Arc<dyn Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::ui::pages::crates::catalog::register_routes();
    let container = support::container_builder()
        .provide(jwt_config)
        .build()
        .await
        .unwrap();
    support::app(container).await
}

#[tokio::test]
async fn crates_index_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true)).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/crates/catalog", &container))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn crates_index_slash_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true)).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/crates/catalog/", &container))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn crates_index_renders_the_page_when_authenticated() {
    let _storage = WithStorageRoot::new();
    let (app, container) = app(jwt_config(false)).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/crates/catalog", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn yank_version_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true)).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/crates/yank",
            &[("name", "my-crate"), ("version", "1.0.0")],
            &container,
        ))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn yank_version_reports_not_found_for_an_unknown_version_when_authenticated() {
    let _storage = WithStorageRoot::new();
    let (app, container) = app(jwt_config(false)).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/crates/yank",
            &[("name", "no-such-crate"), ("version", "1.0.0")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn yank_version_yanks_an_existing_version_when_authenticated() {
    let storage = WithStorageRoot::new();
    write_index_line("my-crate", &sample_record("1.0.0", false));
    let _ = &storage;

    let (app, container) = app(jwt_config(false)).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/crates/yank",
            &[("name", "my-crate"), ("version", "1.0.0")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let (headers, _) = support::parts(resp).await;
    assert!(headers.contains_key("hx-redirect"));
}

#[tokio::test]
async fn unyank_version_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true)).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/crates/unyank",
            &[("name", "my-crate"), ("version", "1.0.0")],
            &container,
        ))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn unyank_version_unyanks_an_existing_version_when_authenticated() {
    let storage = WithStorageRoot::new();
    write_index_line("my-crate", &sample_record("1.0.0", true));
    let _ = &storage;

    let (app, container) = app(jwt_config(false)).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/crates/unyank",
            &[("name", "my-crate"), ("version", "1.0.0")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}
