use crate::support;

use http::{Method, StatusCode};
use warehouse_service::utils::sha256::sha256_hex;

fn write_manifest_and_tag(
    storage: &support::WithDockerStorageRoot,
    repo: &str,
    tag: &str,
    manifest: &[u8],
) {
    let hex = sha256_hex(manifest);
    let digest = format!("sha256:{hex}");

    let manifests_dir = storage.dir.path().join("manifests").join("sha256");
    std::fs::create_dir_all(&manifests_dir).unwrap();
    std::fs::write(manifests_dir.join(&hex), manifest).unwrap();

    let tags_dir = storage.dir.path().join(repo).join("tags");
    std::fs::create_dir_all(&tags_dir).unwrap();
    std::fs::write(tags_dir.join(tag), &digest).unwrap();
}

const MANIFEST_JSON: &str = r#"{"schemaVersion": 2, "mediaType": "application/vnd.docker.distribution.manifest.v2+json", "config": {}, "layers": []}"#;

async fn app() -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::docker::manifest::check_exists::register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

#[tokio::test]
async fn handle_reports_not_found_for_a_missing_tag() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::HEAD,
            "/v2/my-repo/manifests/latest",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn handle_reports_ok_with_the_digest_for_an_existing_tag() {
    let storage = support::WithDockerStorageRoot::new();
    write_manifest_and_tag(&storage, "my-repo", "latest", MANIFEST_JSON.as_bytes());

    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::HEAD,
            "/v2/my-repo/manifests/latest",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (headers, _) = support::parts(resp).await;
    assert!(headers.contains_key("docker-content-digest"));
}
