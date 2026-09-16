use crate::support;

use chrono::Utc;
use http::{Method, StatusCode};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::InMemoryDb;
use quench_db::prelude::Db;
use quench_http::endpoint::Endpoint;
use sqlx::types::Json;
use warehouse_service::domain::artifact::{ArtifactMetadata, ArtifactVersion, Platform};
use warehouse_service::routers::ui::pages::artifacts::catalog::render_artifacts_page;

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

async fn body_html(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.unwrap();
    String::from_utf8(collected.to_bytes().to_vec()).unwrap()
}

fn jwt_config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

fn in_memory_db() -> Db {
    Db::InMemory(InMemoryDb::new())
}

// -----------------------------------------------------------------
// render_artifacts_page
// -----------------------------------------------------------------

#[tokio::test]
async fn render_with_no_versions_shows_the_empty_state() {
    let resp = render_artifacts_page(&[], None, None, None, false);
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(body_html(resp).await.contains("ui_artifact_empty"));
}

#[tokio::test]
async fn render_lists_programs_in_the_tree() {
    let versions = vec![
        version("com.example.one", Platform::Android, 1, false),
        version("com.example.two", Platform::Linux, 5, false),
    ];
    let html = body_html(render_artifacts_page(&versions, None, None, None, false)).await;
    assert!(html.contains("com.example.one"));
    assert!(html.contains("com.example.two"));
    assert!(html.contains("ui_artifact_empty_select_version"));
}

#[tokio::test]
async fn render_selects_the_newest_version_of_the_chosen_program_platform() {
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
    ))
    .await;
    assert!(html.contains("ui_artifact_meta_version_code"));
    assert!(html.contains("3.0"));
}

#[tokio::test]
async fn render_shows_a_yank_button_only_when_the_caller_may_manage() {
    let versions = vec![version("com.example.app", Platform::Android, 7, false)];

    let with_manage = body_html(render_artifacts_page(
        &versions,
        Some("com.example.app"),
        Some("android"),
        Some(7),
        true,
    ))
    .await;
    assert!(with_manage.contains("ui_artifact_yank"));
    assert!(with_manage.contains("/artifacts/yank"));

    let without = body_html(render_artifacts_page(
        &versions,
        Some("com.example.app"),
        Some("android"),
        Some(7),
        false,
    ))
    .await;
    assert!(!without.contains("ui_artifact_yank"));
    assert!(!without.contains("/artifacts/yank"));
}

#[tokio::test]
async fn render_offers_unyank_for_a_yanked_version() {
    let versions = vec![version("com.example.app", Platform::Linux, 9, true)];
    let html = body_html(render_artifacts_page(
        &versions,
        Some("com.example.app"),
        Some("linux"),
        Some(9),
        true,
    ))
    .await;
    assert!(html.contains("ui_artifact_unyank"));
    assert!(html.contains("ui_status_yanked"));
}

// -----------------------------------------------------------------
// HTTP handlers (the artifact feature is off in the test binary, so the
// deterministically reachable branches are login-redirect and disabled)
// -----------------------------------------------------------------

async fn app(
    jwt_config: JwtConfig,
    db: Db,
) -> (
    std::sync::Arc<dyn Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::ui::pages::artifacts::catalog::register_routes();
    let container = support::container_builder()
        .provide(jwt_config)
        .provide(db)
        .build()
        .await
        .unwrap();
    support::app(container).await
}

#[tokio::test]
async fn catalog_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true), in_memory_db()).await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/artifacts/catalog",
            &container,
        ))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn catalog_renders_when_authenticated() {
    let (app, container) = app(jwt_config(false), in_memory_db()).await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/artifacts/catalog",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn yank_redirects_to_login_without_a_session() {
    let (app, container) = app(jwt_config(true), in_memory_db()).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/artifacts/yank",
            &[
                ("program", "com.example.app"),
                ("platform", "android"),
                ("version_code", "1"),
            ],
            &container,
        ))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn yank_is_not_found_when_the_feature_is_disabled() {
    let (app, container) = app(jwt_config(false), in_memory_db()).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/artifacts/yank",
            &[
                ("program", "com.example.app"),
                ("platform", "android"),
                ("version_code", "1"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unyank_is_not_found_when_the_feature_is_disabled() {
    let (app, container) = app(jwt_config(false), in_memory_db()).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/artifacts/unyank",
            &[
                ("program", "com.example.app"),
                ("platform", "android"),
                ("version_code", "1"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
