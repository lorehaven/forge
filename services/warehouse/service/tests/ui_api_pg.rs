//! The file-storage half of the browser client's API (`/ui/api/files`) against a real database. Skipped unless
//! `WAREHOUSE_TEST_DATABASE_URL` points at one with the warehouse module installed (`foundry-service apply -i warehouse`) and
//! a `dev` user in `auth.users`.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, clippy::too_many_lines)]

#[path = "support.rs"]
mod support;

use quench_auth::domain::jwt::JwtConfig;
use quench_core::tmp::TempDir;
use quench_db::prelude::{Database, Db};
use quench_http::http::{Method, StatusCode};
use quench_serde::{Value, json};
use warehouse_core::{Browse, Storages};
use warehouse_service::domain::storage_file;
use warehouse_service::routers::files::dynamic;

type App = (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
);

async fn get(app: &App, path: &str) -> (StatusCode, Value) {
    let resp = app.0.call(support::req(Method::GET, path, &app.1)).await;
    let status = resp.status();
    let text = support::body_text(resp).await;
    (status, json::from_str(&text).unwrap_or(Value::Null))
}

async fn post(app: &App, path: &str, body: &Value) -> (StatusCode, Value) {
    let resp = app
        .0
        .call(support::json_req(Method::POST, path, body, &app.1))
        .await;
    let status = resp.status();
    let text = support::body_text(resp).await;
    (status, json::from_str(&text).unwrap_or(Value::Null))
}

fn browse(body: &Value) -> Browse {
    json::from_str(&json::to_string(body)).unwrap()
}

#[quench_test::test]
async fn storages_are_managed_browsed_and_emptied_through_the_api() {
    let Ok(url) = std::env::var("WAREHOUSE_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect(&url).await.expect("connect");
    db.execute("INSERT INTO auth.users (username, password, roles) VALUES ('dev', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING")
        .await
        .expect("seed the owner");

    // Read once for the whole binary, so set before anything asks.
    let blobs = TempDir::new().expect("tempdir");
    let shared = TempDir::new().expect("tempdir");
    quench_core::env::set("FEATURE_FILES_ENABLED", "true");
    quench_core::env::set("DYNAMIC_STORAGE_ROOT", blobs.path().to_str().unwrap());
    quench_core::env::set(
        "FILE_STORAGES",
        format!("shared={}", shared.path().to_str().unwrap()),
    );
    std::fs::create_dir_all(shared.path().join("sub")).unwrap();
    std::fs::write(shared.path().join("sub/note.txt"), b"hi").unwrap();
    std::fs::write(shared.path().join("top.txt"), b"top").unwrap();

    warehouse_service::routers::ui::register_routes();
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = false;
    let container = support::container_builder()
        .provide(config)
        .provide(db.clone())
        .build()
        .await
        .unwrap();
    let app = support::app(container);
    let name = format!("t{}", quench_core::uuid::Uuid::new_v4().simple());

    // Creating: checked, then stored.
    for (body, key) in [
        (
            json!({"name": "a b", "owner": "dev", "sync_enabled": false}),
            "api_error_invalid_storage_name",
        ),
        (
            json!({"name": name, "owner": " ", "sync_enabled": false}),
            "api_error_storage_owner_required",
        ),
        (
            json!({"name": "shared", "owner": "dev", "sync_enabled": false}),
            "api_error_storage_name_static_clash",
        ),
        (
            json!({"name": name, "owner": "dev", "quota_bytes": -1, "sync_enabled": false}),
            "api_error_invalid_quota",
        ),
        (
            json!({"name": name, "owner": "dev", "max_file_bytes": -1, "sync_enabled": false}),
            "api_error_invalid_max_file",
        ),
        (
            json!({"name": name, "owner": "nobody-here", "sync_enabled": false}),
            "api_error_storage_owner_unknown",
        ),
    ] {
        let (status, answer) = post(&app, "/ui/api/files/storages", &body).await;
        assert!(status.is_client_error(), "{key}: {status}");
        assert_eq!(answer["error"], key, "{status}");
    }
    let (status, created) = post(
        &app,
        "/ui/api/files/storages",
        &json!({"name": name, "owner": "dev", "quota_bytes": 10_000, "max_file_bytes": 500, "sync_enabled": true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created:?}");
    assert_eq!(created["quota_bytes"], 10_000);
    assert!(created["created"].as_str().unwrap().ends_with("UTC"));
    let (status, answer) = post(
        &app,
        "/ui/api/files/storages",
        &json!({"name": name, "owner": "dev", "sync_enabled": false}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(answer["error"], "api_error_storage_exists");

    // Listing: the configured one and the new one.
    let (_, listed) = get(&app, "/ui/api/files/storages").await;
    let storages: Storages = json::from_str(&json::to_string(&listed)).unwrap();
    assert_eq!(storages.fixed[0].name, "shared");
    assert!(
        storages
            .dynamic_named(&name)
            .is_some_and(|s| s.sync_enabled)
    );

    // Editing: a new quota, the file limit cleared.
    let (status, edited) = post(
        &app,
        &format!("/ui/api/files/storages/{name}/edit"),
        &json!({"quota_bytes": 20_000, "clear_max_file": true, "sync_enabled": false}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited:?}");
    assert_eq!(edited["quota_bytes"], 20_000);
    assert_eq!(edited["max_file_bytes"], Value::Null);
    assert_eq!(edited["sync_enabled"], false);
    let (status, answer) = post(
        &app,
        "/ui/api/files/storages/nope-nope/edit",
        &json!({"clear_max_file": false, "sync_enabled": false}),
    )
    .await;
    assert_eq!(
        (status, answer["error"].as_str()),
        (StatusCode::NOT_FOUND, Some("api_error_storage_not_found"))
    );

    // Browsing a database storage.
    let sha = "cd".repeat(32);
    let blob = dynamic::blob_path(blobs.path(), &sha);
    std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
    let staging = blobs.path().join("staging");
    std::fs::write(&staging, b"hello").unwrap();
    storage_file::put_file(&db, &name, "docs/a.txt", &sha, 5, &staging, &blob)
        .await
        .expect("put");
    let (status, body) = get(&app, &format!("/ui/api/files/browse?storage={name}")).await;
    assert_eq!(status, StatusCode::OK);
    let listing = browse(&body);
    assert_eq!(listing.access, Browse::READABLE);
    assert_eq!(listing.files.len(), 1);
    assert_eq!(
        (listing.files[0].path.as_str(), listing.files[0].size),
        ("docs/a.txt", Some(5))
    );

    // Browsing a configured storage walks its directory.
    let (_, body) = get(&app, "/ui/api/files/browse?storage=shared").await;
    let mut paths: Vec<String> = browse(&body).files.into_iter().map(|f| f.path).collect();
    paths.sort();
    assert_eq!(paths, ["sub/note.txt", "top.txt"]);

    // Removing a file: from the database store, then from the directory.
    let (status, answer) = post(
        &app,
        "/ui/api/files/delete-file",
        &json!({"storage": name, "path": "../escape"}),
    )
    .await;
    assert_eq!(
        (status, answer["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("api_error_invalid_path"))
    );
    let (status, _) = post(
        &app,
        "/ui/api/files/delete-file",
        &json!({"storage": name, "path": "docs/a.txt"}),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, answer) = post(
        &app,
        "/ui/api/files/delete-file",
        &json!({"storage": name, "path": "docs/a.txt"}),
    )
    .await;
    assert_eq!(
        (status, answer["error"].as_str()),
        (StatusCode::NOT_FOUND, Some("api_error_file_not_found"))
    );
    let (status, _) = post(
        &app,
        "/ui/api/files/delete-file",
        &json!({"storage": "shared", "path": "sub/note.txt"}),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!shared.path().join("sub/note.txt").exists());
    assert!(shared.path().join("top.txt").exists());
    let (status, answer) = post(
        &app,
        "/ui/api/files/delete-file",
        &json!({"storage": "ghost", "path": "x.txt"}),
    )
    .await;
    assert_eq!(
        (status, answer["error"].as_str()),
        (StatusCode::NOT_FOUND, Some("api_error_storage_not_found"))
    );

    // Removing the storage takes its files.
    storage_file::put_file(&db, &name, "again.txt", &sha, 5, &staging, &blob)
        .await
        .ok();
    let (status, _) = post(
        &app,
        &format!("/ui/api/files/storages/{name}/delete"),
        &json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, listed) = get(&app, "/ui/api/files/storages").await;
    let storages: Storages = json::from_str(&json::to_string(&listed)).unwrap();
    assert!(storages.dynamic_named(&name).is_none());
    let (status, _) = post(
        &app,
        &format!("/ui/api/files/storages/{name}/delete"),
        &json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, body) = get(&app, &format!("/ui/api/files/browse?storage={name}")).await;
    assert_eq!(browse(&body).access, Browse::MISSING);
}

quench_test::main!();
