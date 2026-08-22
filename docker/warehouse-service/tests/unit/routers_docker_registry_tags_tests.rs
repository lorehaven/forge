use crate::support;

use actix_web::{App, test};
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::registry::tags::handle;

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

#[actix_web::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = with_tags("my-repo", &["latest"]);
    let app = test::init_service(App::new().service(handle)).await;
    let req = test::TestRequest::get()
        .uri("/..%2fetc/tags/list")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_reports_not_found_for_an_unknown_repository() {
    let _storage = with_tags("my-repo", &["latest"]);
    let app = test::init_service(App::new().service(handle)).await;
    let req = test::TestRequest::get()
        .uri("/no-such-repo/tags/list")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn handle_lists_tags_for_a_known_repository() {
    let _storage = with_tags("my-repo", &["1.0.0", "2.0.0"]);
    let app = test::init_service(App::new().service(handle)).await;
    let req = test::TestRequest::get()
        .uri("/my-repo/tags/list")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

    let body: TagsResponseForTest = test::read_body_json(resp).await;
    assert_eq!(body.name, "my-repo");
    assert_eq!(body.tags, vec!["2.0.0".to_string(), "1.0.0".to_string()]);
}

#[actix_web::test]
async fn handle_paginates_with_n_and_sets_a_link_header_when_more_remain() {
    let _storage = with_tags("my-repo", &["1.0.0", "2.0.0", "3.0.0"]);
    let app = test::init_service(App::new().service(handle)).await;
    let req = test::TestRequest::get()
        .uri("/my-repo/tags/list?n=2")
        .to_request();
    let resp = test::call_service(&app, req).await;
    let link = resp.headers().get("Link").unwrap().to_str().unwrap();
    assert!(link.contains("my-repo/tags/list"), "{link}");

    let body: TagsResponseForTest = test::read_body_json(resp).await;
    assert_eq!(body.tags.len(), 2);
}
