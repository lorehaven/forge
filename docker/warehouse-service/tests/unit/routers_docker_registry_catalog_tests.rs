use crate::support;

use actix_web::{App, test};
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::registry::catalog::handle;

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

#[actix_web::test]
async fn handle_lists_every_repository_when_under_the_default_page_size() {
    let _storage = with_repos(&["alpha", "beta"]);
    let app = test::init_service(App::new().service(handle)).await;
    let req = test::TestRequest::get().uri("/_catalog").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert!(!resp.headers().contains_key("Link"));

    let body: CatalogResponseForTest = test::read_body_json(resp).await;
    assert_eq!(
        body.repositories,
        vec!["alpha".to_string(), "beta".to_string()]
    );
}

#[actix_web::test]
async fn handle_paginates_with_n_and_sets_a_link_header_when_more_remain() {
    let _storage = with_repos(&["alpha", "beta", "gamma"]);
    let app = test::init_service(App::new().service(handle)).await;
    let req = test::TestRequest::get().uri("/_catalog?n=2").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let link = resp.headers().get("Link").unwrap().to_str().unwrap();
    assert!(link.contains("last=beta"), "{link}");

    let body: CatalogResponseForTest = test::read_body_json(resp).await;
    assert_eq!(
        body.repositories,
        vec!["alpha".to_string(), "beta".to_string()]
    );
}

#[actix_web::test]
async fn handle_resumes_after_the_last_seen_repository() {
    let _storage = with_repos(&["alpha", "beta", "gamma"]);
    let app = test::init_service(App::new().service(handle)).await;
    let req = test::TestRequest::get()
        .uri("/_catalog?last=alpha")
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: CatalogResponseForTest = test::read_body_json(resp).await;
    assert_eq!(
        body.repositories,
        vec!["beta".to_string(), "gamma".to_string()]
    );
}
