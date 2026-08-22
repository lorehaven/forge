use crate::support;

use actix_web::test as actix_test;
use support::WithCratesStorageRoot as WithStorageRoot;
use warehouse_service::routers::crates::owners::{Owner, add, list, remove};

fn publish_crate(storage: &WithStorageRoot, name: &str) {
    std::fs::create_dir_all(storage.dir.path().join(name)).unwrap();
}

fn owners_json(storage: &WithStorageRoot, name: &str) -> serde_json::Value {
    let data = std::fs::read(storage.dir.path().join(name).join("owners.json")).unwrap();
    serde_json::from_slice(&data).unwrap()
}

fn app() -> impl actix_web::dev::HttpServiceFactory {
    (list, add, remove)
}

#[actix_web::test]
async fn list_reports_not_found_for_an_unpublished_crate() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(app())).await;
    let req = actix_test::TestRequest::get()
        .uri("/no-such-crate/owners")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn list_is_empty_for_a_published_crate_with_no_owners_file() {
    let storage = WithStorageRoot::new();
    publish_crate(&storage, "my-crate");

    let app = actix_test::init_service(actix_web::App::new().service(app())).await;
    let req = actix_test::TestRequest::get()
        .uri("/my-crate/owners")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["users"], serde_json::json!([]));
}

#[actix_web::test]
async fn add_rejects_an_empty_user_list() {
    let storage = WithStorageRoot::new();
    publish_crate(&storage, "my-crate");

    let app = actix_test::init_service(actix_web::App::new().service(app())).await;
    let req = actix_test::TestRequest::put()
        .uri("/my-crate/owners")
        .set_json(serde_json::json!({ "users": [] }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn add_assigns_sequential_ids_and_skips_case_insensitive_duplicates() {
    let storage = WithStorageRoot::new();
    publish_crate(&storage, "my-crate");

    let app = actix_test::init_service(actix_web::App::new().service(app())).await;
    let req = actix_test::TestRequest::put()
        .uri("/my-crate/owners")
        .set_json(serde_json::json!({ "users": ["alice", "bob"] }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

    // Adding "ALICE" (different case) and a genuinely new user "carol".
    let req = actix_test::TestRequest::put()
        .uri("/my-crate/owners")
        .set_json(serde_json::json!({ "users": ["ALICE", "carol"] }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

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

#[actix_web::test]
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

    let app = actix_test::init_service(actix_web::App::new().service(app())).await;
    let req = actix_test::TestRequest::delete()
        .uri("/my-crate/owners")
        .set_json(serde_json::json!({ "users": ["ALICE"] }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

    let owners = owners_json(&storage, "my-crate");
    let users = owners.as_array().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["login"], "bob");
}

#[actix_web::test]
async fn remove_rejects_an_empty_user_list() {
    let storage = WithStorageRoot::new();
    publish_crate(&storage, "my-crate");

    let app = actix_test::init_service(actix_web::App::new().service(app())).await;
    let req = actix_test::TestRequest::delete()
        .uri("/my-crate/owners")
        .set_json(serde_json::json!({ "users": [] }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn add_rejects_an_invalid_crate_name() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(app())).await;
    let req = actix_test::TestRequest::put()
        .uri("/..%2fetc/owners")
        .set_json(serde_json::json!({ "users": ["alice"] }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}
