use crate::support;
use http::{Method, StatusCode};
use quench_http::endpoint::Endpoint;
use support::WithCratesStorageRoot as WithStorageRoot;
use warehouse_service::routers::crates::owners::{self, Owner};

fn publish_crate(storage: &WithStorageRoot, name: &str) {
    std::fs::create_dir_all(storage.dir.path().join(name)).unwrap();
}

fn owners_json(storage: &WithStorageRoot, name: &str) -> serde_json::Value {
    let data = std::fs::read(storage.dir.path().join(name).join("owners.json")).unwrap();
    serde_json::from_slice(&data).unwrap()
}

async fn app() -> (
    std::sync::Arc<dyn Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    owners::register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

#[tokio::test]
async fn list_reports_not_found_for_an_unpublished_crate() {
    let _storage = WithStorageRoot::new();
    let (app, container) = app().await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/crates/no-such-crate/owners",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_is_empty_for_a_published_crate_with_no_owners_file() {
    let storage = WithStorageRoot::new();
    publish_crate(&storage, "my-crate");
    let (app, container) = app().await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/crates/my-crate/owners",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = support::json_body(resp).await;
    assert_eq!(body["users"], serde_json::json!([]));
}

#[tokio::test]
async fn add_rejects_an_empty_user_list() {
    let storage = WithStorageRoot::new();
    publish_crate(&storage, "my-crate");
    let (app, container) = app().await;

    let resp = app
        .call(support::json_req(
            Method::PUT,
            "/api/v1/crates/my-crate/owners",
            serde_json::json!({ "users": [] }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn add_assigns_sequential_ids_and_skips_case_insensitive_duplicates() {
    let storage = WithStorageRoot::new();
    publish_crate(&storage, "my-crate");
    let (app, container) = app().await;

    let resp = app
        .call(support::json_req(
            Method::PUT,
            "/api/v1/crates/my-crate/owners",
            serde_json::json!({ "users": ["alice", "bob"] }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Adding "ALICE" (different case) and a genuinely new user "carol".
    let resp = app
        .call(support::json_req(
            Method::PUT,
            "/api/v1/crates/my-crate/owners",
            serde_json::json!({ "users": ["ALICE", "carol"] }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let owners = owners_json(&storage, "my-crate");
    let users = owners.as_array().unwrap();
    assert_eq!(users.len(), 3);
    assert_eq!(users[0]["login"], "alice");
    assert_eq!(users[0]["id"], 1);
    assert_eq!(users[1]["login"], "bob");
    assert_eq!(users[1]["id"], 2);
    assert_eq!(users[2]["login"], "carol");
    assert_eq!(users[2]["id"], 3);
}

#[tokio::test]
async fn remove_deletes_owners_case_insensitively() {
    let storage = WithStorageRoot::new();
    publish_crate(&storage, "my-crate");
    std::fs::write(
        storage.dir.path().join("my-crate").join("owners.json"),
        serde_json::to_vec(&[
            Owner {
                id: 1,
                login: "alice".to_string(),
                name: None,
            },
            Owner {
                id: 2,
                login: "bob".to_string(),
                name: None,
            },
        ])
        .unwrap(),
    )
    .unwrap();
    let (app, container) = app().await;

    let resp = app
        .call(support::json_req(
            Method::DELETE,
            "/api/v1/crates/my-crate/owners",
            serde_json::json!({ "users": ["ALICE"] }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let owners = owners_json(&storage, "my-crate");
    let users = owners.as_array().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["login"], "bob");
}

#[tokio::test]
async fn remove_rejects_an_empty_user_list() {
    let storage = WithStorageRoot::new();
    publish_crate(&storage, "my-crate");
    let (app, container) = app().await;

    let resp = app
        .call(support::json_req(
            Method::DELETE,
            "/api/v1/crates/my-crate/owners",
            serde_json::json!({ "users": [] }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn add_rejects_an_invalid_crate_name() {
    let _storage = WithStorageRoot::new();
    let (app, container) = app().await;

    let resp = app
        .call(support::json_req(
            Method::PUT,
            "/api/v1/crates/..%2fetc/owners",
            serde_json::json!({ "users": ["alice"] }),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
