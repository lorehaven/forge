use crate::support;

use http::Method;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::register_routes;

fn with_repos(repos: &[&str]) -> WithStorageRoot {
    let storage = WithStorageRoot::new();
    for repo in repos {
        std::fs::create_dir_all(storage.dir.path().join(repo).join("tags")).unwrap();
    }
    storage
}

#[derive(serde::Deserialize)]
struct CatalogResponseForTest {
    repositories: Vec<String>,
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
async fn handle_lists_every_repository_when_under_the_default_page_size() {
    let _storage = with_repos(&["alpha", "beta"]);
    let (app, container) = app().await;
    let req = support::req(Method::GET, "/v2/_catalog", &container);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), http::StatusCode::OK);
    let (headers, body) = support::parts(resp).await;
    assert!(!headers.contains_key("link"));

    let body: CatalogResponseForTest = serde_json::from_str(&body).unwrap();
    assert_eq!(
        body.repositories,
        vec!["alpha".to_string(), "beta".to_string()]
    );
}

#[tokio::test]
async fn handle_paginates_with_n_and_sets_a_link_header_when_more_remain() {
    let _storage = with_repos(&["alpha", "beta", "gamma"]);
    let (app, container) = app().await;
    let req = support::req(Method::GET, "/v2/_catalog?n=2", &container);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), http::StatusCode::OK);
    let (headers, body) = support::parts(resp).await;
    let link = headers.get("link").unwrap().to_str().unwrap();
    assert!(link.contains("last=beta"), "{link}");

    let body: CatalogResponseForTest = serde_json::from_str(&body).unwrap();
    assert_eq!(
        body.repositories,
        vec!["alpha".to_string(), "beta".to_string()]
    );
}

#[tokio::test]
async fn handle_resumes_after_the_last_seen_repository() {
    let _storage = with_repos(&["alpha", "beta", "gamma"]);
    let (app, container) = app().await;
    let req = support::req(Method::GET, "/v2/_catalog?last=alpha", &container);
    let resp = app.call(req).await;
    let body = support::json_body(resp).await;
    let body: CatalogResponseForTest = serde_json::from_value(body).unwrap();
    assert_eq!(
        body.repositories,
        vec!["beta".to_string(), "gamma".to_string()]
    );
}
