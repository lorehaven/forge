use crate::support;

use http::{Method, StatusCode};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::ui::pages::docker::catalog::{
    DeleteImageModalQuery, empty_delete_image_modal_html, render_catalog_page,
    render_delete_image_modal,
};

fn write_tag(
    storage: &WithStorageRoot,
    repo: &str,
    tag: &str,
    digest: &str,
    manifest_json: Option<&str>,
) {
    let tags_dir = storage.dir.path().join(repo).join("tags");
    std::fs::create_dir_all(&tags_dir).unwrap();
    std::fs::write(tags_dir.join(tag), digest).unwrap();

    if let (Some(hex), Some(json)) = (digest.strip_prefix("sha256:"), manifest_json) {
        let manifests_dir = storage.dir.path().join("manifests").join("sha256");
        std::fs::create_dir_all(&manifests_dir).unwrap();
        std::fs::write(manifests_dir.join(hex), json).unwrap();
    }
}

const DIGEST: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

async fn body_html(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.unwrap();
    String::from_utf8(collected.to_bytes().to_vec()).unwrap()
}

// -----------------------------------------------------------------
// render_catalog_page
// -----------------------------------------------------------------

#[tokio::test]
async fn render_catalog_page_with_no_repositories_renders_ok() {
    let _storage = WithStorageRoot::new();
    let resp = render_catalog_page(None, None);
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_html(resp).await;
    assert!(html.contains("ui_repositories"));
}

#[tokio::test]
async fn render_catalog_page_lists_repositories_in_the_tree() {
    let storage = WithStorageRoot::new();
    write_tag(&storage, "my/repo", "latest", DIGEST, None);
    write_tag(&storage, "other-repo", "v1", DIGEST, None);

    let resp = render_catalog_page(None, None);
    let html = body_html(resp).await;
    assert!(html.contains("other-repo"));
    assert!(html.contains("my"));
}

#[tokio::test]
async fn render_catalog_page_with_an_unknown_selected_repo_ignores_the_selection() {
    let storage = WithStorageRoot::new();
    write_tag(&storage, "my-repo", "latest", DIGEST, None);

    let resp = render_catalog_page(Some("no-such-repo".to_string()), None);
    let html = body_html(resp).await;
    assert!(html.contains("ui_empty_select_tag"));
}

#[tokio::test]
async fn render_catalog_page_with_a_selected_repo_and_tag_shows_metadata() {
    let storage = WithStorageRoot::new();
    let manifest = r#"{"mediaType": "application/vnd.oci.image.manifest.v1+json"}"#;
    write_tag(&storage, "my-repo", "latest", DIGEST, Some(manifest));

    let resp = render_catalog_page(Some("my-repo".to_string()), Some("latest".to_string()));
    let html = body_html(resp).await;
    assert!(html.contains("ui_meta_for") || html.contains("latest"));
    assert!(html.contains(DIGEST));
    assert!(html.contains("ui_delete_image"));
}

#[tokio::test]
async fn render_catalog_page_with_a_selected_repo_but_unknown_tag_shows_the_tag_list_without_metadata()
 {
    let storage = WithStorageRoot::new();
    write_tag(&storage, "my-repo", "latest", DIGEST, None);

    let resp = render_catalog_page(Some("my-repo".to_string()), Some("no-such-tag".to_string()));
    let html = body_html(resp).await;
    assert!(html.contains("ui_empty_select_tag"));
}

// -----------------------------------------------------------------
// render_delete_image_modal / empty_delete_image_modal_html
// -----------------------------------------------------------------

#[test]
fn render_delete_image_modal_with_a_tag_shows_repo_colon_tag() {
    let query = DeleteImageModalQuery {
        repository: "my-repo".to_string(),
        tag: Some("latest".to_string()),
        digest: DIGEST.to_string(),
    };
    let html = render_delete_image_modal(&query);
    assert!(html.contains("my-repo:latest"));
    assert!(html.contains("confirm-delete-image-modal"));
}

#[test]
fn render_delete_image_modal_without_a_tag_shows_the_repo_alone() {
    let query = DeleteImageModalQuery {
        repository: "my-repo".to_string(),
        tag: None,
        digest: DIGEST.to_string(),
    };
    let html = render_delete_image_modal(&query);
    assert!(html.contains("my-repo"));
    assert!(!html.contains("my-repo:"));
}

#[test]
fn render_delete_image_modal_with_an_empty_tag_shows_the_repo_alone() {
    let query = DeleteImageModalQuery {
        repository: "my-repo".to_string(),
        tag: Some(String::new()),
        digest: DIGEST.to_string(),
    };
    let html = render_delete_image_modal(&query);
    assert!(!html.contains("my-repo:"));
}

#[test]
fn empty_delete_image_modal_html_has_the_modal_id_but_no_content() {
    let html = empty_delete_image_modal_html();
    assert!(html.contains("confirm-delete-image-modal"));
    assert!(!html.contains("ui_modal_delete_title"));
}

// -----------------------------------------------------------------
// HTTP handlers - auth gate only (see routers_ui_pages_docker_tags_tests
// for the same "no GATEHOUSE" / auth_enabled=false pattern)
// -----------------------------------------------------------------

fn jwt_config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

async fn app(
    jwt_config: JwtConfig,
) -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::ui::pages::docker::catalog::register_routes();
    let container = support::container_builder()
        .provide(jwt_config)
        .build()
        .await
        .unwrap();
    support::app(container).await
}

#[tokio::test]
async fn docker_catalog_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true)).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/docker/catalog", &container))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn docker_catalog_slash_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true)).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/docker/catalog/", &container))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn docker_catalog_renders_the_page_when_authenticated() {
    let _storage = WithStorageRoot::new();
    let (app, container) = app(jwt_config(false)).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/docker/catalog", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn delete_image_modal_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true)).await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/docker/delete-image-modal?repository=my-repo&digest=sha256:abc",
            &container,
        ))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn delete_image_modal_renders_when_authenticated() {
    let (app, container) = app(jwt_config(false)).await;
    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/ui/docker/delete-image-modal?repository=my-repo&digest={DIGEST}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn empty_delete_image_modal_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true)).await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/docker/delete-image-modal/empty",
            &container,
        ))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn empty_delete_image_modal_renders_when_authenticated() {
    let (app, container) = app(jwt_config(false)).await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/docker/delete-image-modal/empty",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn delete_image_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true)).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/docker/delete-image",
            &[("repository", "my-repo"), ("digest", DIGEST)],
            &container,
        ))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn delete_image_rejects_an_invalid_digest_when_authenticated() {
    let _storage = WithStorageRoot::new();
    let (app, container) = app(jwt_config(false)).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/docker/delete-image",
            &[("repository", "my-repo"), ("digest", "not-a-digest")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_image_reports_not_found_for_a_missing_manifest() {
    let _storage = WithStorageRoot::new();
    let (app, container) = app(jwt_config(false)).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/docker/delete-image",
            &[("repository", "my-repo"), ("digest", DIGEST)],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
