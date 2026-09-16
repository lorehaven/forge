use crate::support;

use http::{Method, StatusCode};

const DIGEST: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn write_manifest(storage: &support::WithDockerStorageRoot) -> std::path::PathBuf {
    let hex = DIGEST.strip_prefix("sha256:").unwrap();
    let dir = storage.dir.path().join("manifests").join("sha256");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(hex);
    std::fs::write(&path, b"{}").unwrap();
    path
}

fn write_tag(
    storage: &support::WithDockerStorageRoot,
    repo: &str,
    tag: &str,
    digest: &str,
) -> std::path::PathBuf {
    let dir = storage.dir.path().join(repo).join("tags");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(tag);
    std::fs::write(&path, digest).unwrap();
    path
}

async fn app() -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::docker::manifest::delete_image::register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

#[tokio::test]
async fn handle_rejects_a_non_digest_reference() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::DELETE,
            "/v2/my-repo/manifests/latest",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::DELETE,
            &format!("/v2/..%2fetc/manifests/{DIGEST}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handle_reports_not_found_for_an_unknown_manifest() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::DELETE,
            &format!("/v2/my-repo/manifests/{DIGEST}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn handle_deletes_the_manifest_and_every_tag_pointing_at_it() {
    let storage = support::WithDockerStorageRoot::new();
    let manifest_path = write_manifest(&storage);
    let matching_tag = write_tag(&storage, "my-repo", "latest", DIGEST);
    let other_tag = write_tag(
        &storage,
        "my-repo",
        "v1",
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    );

    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::DELETE,
            &format!("/v2/my-repo/manifests/{DIGEST}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    assert!(!manifest_path.exists());
    assert!(
        !matching_tag.exists(),
        "tag pointing at the deleted digest should be removed"
    );
    assert!(other_tag.exists(), "unrelated tag should be left alone");
}
