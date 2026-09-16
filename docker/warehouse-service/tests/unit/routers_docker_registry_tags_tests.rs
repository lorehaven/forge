use crate::support;

use http::Method;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::register_routes;

fn with_tags(repo: &str, tags: &[&str]) -> WithStorageRoot {
    let storage = WithStorageRoot::new();
    let tags_dir = storage.dir.path().join(repo).join("tags");
    std::fs::create_dir_all(&tags_dir).unwrap();
    for tag in tags {
        std::fs::write(
            tags_dir.join(tag),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        )
        .unwrap();
    }
    storage
}

#[derive(serde::Deserialize)]
struct TagsResponseForTest {
    name: String,
    tags: Vec<String>,
}

async fn app() -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

#[tokio::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = with_tags("my-repo", &["latest"]);
    let (app, container) = app().await;
    let req = support::req(Method::GET, "/v2/..%2fetc/tags/list", &container);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handle_reports_not_found_for_an_unknown_repository() {
    let _storage = with_tags("my-repo", &["latest"]);
    let (app, container) = app().await;
    let req = support::req(Method::GET, "/v2/no-such-repo/tags/list", &container);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn handle_lists_tags_for_a_known_repository() {
    let _storage = with_tags("my-repo", &["1.0.0", "2.0.0"]);
    let (app, container) = app().await;
    let req = support::req(Method::GET, "/v2/my-repo/tags/list", &container);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), http::StatusCode::OK);

    let body = support::json_body(resp).await;
    let body: TagsResponseForTest = serde_json::from_value(body).unwrap();
    assert_eq!(body.name, "my-repo");
    assert_eq!(body.tags, vec!["2.0.0".to_string(), "1.0.0".to_string()]);
}

#[tokio::test]
async fn handle_paginates_with_n_and_sets_a_link_header_when_more_remain() {
    let _storage = with_tags("my-repo", &["1.0.0", "2.0.0", "3.0.0"]);
    let (app, container) = app().await;
    let req = support::req(Method::GET, "/v2/my-repo/tags/list?n=2", &container);
    let resp = app.call(req).await;
    let (headers, body) = support::parts(resp).await;
    let link = headers.get("link").unwrap().to_str().unwrap();
    assert!(link.contains("my-repo/tags/list"), "{link}");

    let body: TagsResponseForTest = serde_json::from_str(&body).unwrap();
    assert_eq!(body.tags.len(), 2);
}
