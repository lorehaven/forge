use crate::support;

use actix_web::body::MessageBody;
use actix_web::{App, test as actix_test, web};
use quench_auth::prelude::JwtConfig;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::ui::pages::docker::catalog::{
    DeleteImageModalQuery, delete_image, delete_image_modal, docker_catalog, docker_catalog_slash,
    empty_delete_image_modal, empty_delete_image_modal_html, render_catalog_page,
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

fn body_html(resp: actix_web::HttpResponse) -> String {
    let body = resp.into_body().try_into_bytes().unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

// -----------------------------------------------------------------
// render_catalog_page
// -----------------------------------------------------------------

#[test]
fn render_catalog_page_with_no_repositories_renders_ok() {
    let _storage = WithStorageRoot::new();
    let resp = render_catalog_page(None, None);
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let html = body_html(resp);
    assert!(html.contains("ui_repositories"));
}

#[test]
fn render_catalog_page_lists_repositories_in_the_tree() {
    let storage = WithStorageRoot::new();
    write_tag(&storage, "my/repo", "latest", DIGEST, None);
    write_tag(&storage, "other-repo", "v1", DIGEST, None);

    let resp = render_catalog_page(None, None);
    let html = body_html(resp);
    assert!(html.contains("other-repo"));
    assert!(html.contains("my"));
}

#[test]
fn render_catalog_page_with_an_unknown_selected_repo_ignores_the_selection() {
    let storage = WithStorageRoot::new();
    write_tag(&storage, "my-repo", "latest", DIGEST, None);

    let resp = render_catalog_page(Some("no-such-repo".to_string()), None);
    let html = body_html(resp);
    assert!(html.contains("ui_empty_select_tag"));
}

#[test]
fn render_catalog_page_with_a_selected_repo_and_tag_shows_metadata() {
    let storage = WithStorageRoot::new();
    let manifest = r#"{"mediaType": "application/vnd.oci.image.manifest.v1+json"}"#;
    write_tag(&storage, "my-repo", "latest", DIGEST, Some(manifest));

    let resp = render_catalog_page(Some("my-repo".to_string()), Some("latest".to_string()));
    let html = body_html(resp);
    assert!(html.contains("ui_meta_for") || html.contains("latest"));
    assert!(html.contains(DIGEST));
    assert!(html.contains("ui_delete_image"));
}

#[test]
fn render_catalog_page_with_a_selected_repo_but_unknown_tag_shows_the_tag_list_without_metadata() {
    let storage = WithStorageRoot::new();
    write_tag(&storage, "my-repo", "latest", DIGEST, None);

    let resp = render_catalog_page(Some("my-repo".to_string()), Some("no-such-tag".to_string()));
    let html = body_html(resp);
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

#[actix_web::test]
async fn docker_catalog_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .service(docker_catalog),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/docker/catalog")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn docker_catalog_slash_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .service(docker_catalog_slash),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/docker/catalog/")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn docker_catalog_renders_the_page_when_authenticated() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .service(docker_catalog),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/docker/catalog")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn delete_image_modal_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .service(delete_image_modal),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/docker/delete-image-modal?repository=my-repo&digest=sha256:abc")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn delete_image_modal_renders_when_authenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .service(delete_image_modal),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri(&format!(
            "/docker/delete-image-modal?repository=my-repo&digest={DIGEST}"
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn empty_delete_image_modal_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .service(empty_delete_image_modal),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/docker/delete-image-modal/empty")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn empty_delete_image_modal_renders_when_authenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .service(empty_delete_image_modal),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/docker/delete-image-modal/empty")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn delete_image_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .service(delete_image),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/docker/delete-image")
        .set_form([("repository", "my-repo"), ("digest", DIGEST)])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn delete_image_rejects_an_invalid_digest_when_authenticated() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .service(delete_image),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/docker/delete-image")
        .set_form([("repository", "my-repo"), ("digest", "not-a-digest")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn delete_image_reports_not_found_for_a_missing_manifest() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .service(delete_image),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/docker/delete-image")
        .set_form([("repository", "my-repo"), ("digest", DIGEST)])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}
