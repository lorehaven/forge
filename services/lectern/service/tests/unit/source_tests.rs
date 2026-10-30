use lectern_service::source::{Source, WarehouseSource};
use quench_test::mock::matchers::{header, method, path, query_param};
use quench_test::mock::{Mock, MockServer, ResponseTemplate};

#[quench_test::test]
async fn the_books_are_the_directories_at_the_root() {
    let warehouse = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/files/docs"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(quench_serde::json!({
                "storage": "docs", "prefix": "",
                "entries": [
                    {"name": "forge", "path": "forge", "kind": "directory"},
                    {"name": ".trash", "path": ".trash", "kind": "directory"},
                    {"name": "notes.txt", "path": "notes.txt", "kind": "file", "size": 3},
                    {"name": "quench", "path": "quench", "kind": "directory"},
                ]
            })),
        )
        .mount(&warehouse)
        .await;

    let source = WarehouseSource::new(&warehouse.uri(), "docs", None, true);
    assert_eq!(
        source.books().await.unwrap(),
        vec!["forge".to_string(), "quench".to_string()]
    );
}

#[quench_test::test]
async fn a_file_is_read_by_path_and_a_missing_one_is_none() {
    let warehouse = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/files/docs/file"))
        .and(query_param("path", "forge/latest.json"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"{}".to_vec()))
        .mount(&warehouse)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/files/docs/file"))
        .and(query_param("path", "forge/gone.json"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&warehouse)
        .await;

    let source = WarehouseSource::new(&warehouse.uri(), "docs", None, true);
    assert_eq!(
        source.read("forge/latest.json").await.unwrap(),
        Some(b"{}".to_vec())
    );
    assert_eq!(source.read("forge/gone.json").await.unwrap(), None);
}

#[quench_test::test]
async fn requests_carry_the_services_own_token_from_the_client_credentials_grant() {
    let warehouse = MockServer::start().await;
    let gatehouse = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            quench_serde::json!({"access_token": "t-123", "expires_in": 600, "token_type": "Bearer"}),
        ))
        .expect(1)
        .mount(&gatehouse)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/files/docs"))
        .and(header("authorization", "Bearer t-123"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(quench_serde::json!({"entries": []})),
        )
        .mount(&warehouse)
        .await;

    let source = WarehouseSource::new(
        &warehouse.uri(),
        "docs",
        Some((&gatehouse.uri(), "lectern-warehouse", "its-secret")),
        true,
    );
    assert!(source.books().await.unwrap().is_empty());
    // the token is kept, not asked for again (the mock allows one exchange)
    assert!(source.books().await.unwrap().is_empty());
}

#[quench_test::test]
async fn a_refusal_is_an_error_not_an_empty_answer() {
    let warehouse = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/files/docs"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&warehouse)
        .await;
    let source = WarehouseSource::new(&warehouse.uri(), "docs", None, true);
    let error = source.books().await.unwrap_err();
    assert!(error.to_string().contains("403"), "{error}");
}
