//! The dynamic-storage SQL (quota, blob reference counts, sync log) against a real database. Skipped
//! unless `WAREHOUSE_TEST_DATABASE_URL` points at one with the warehouse module installed
//! (`foundry-service apply -i warehouse`).

use quench_core::tmp::TempDir;
use quench_db::prelude::{Database, Db};
use warehouse_service::domain::storage::{self, NewStorage, StorageUpdate};
use warehouse_service::domain::storage_file;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn storages_files_blobs_and_the_sync_log_round_trip() {
    let Ok(url) = std::env::var("WAREHOUSE_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect(&url).await.expect("connect");
    let name = format!("t{}", quench_core::uuid::Uuid::new_v4().simple());
    let dir = TempDir::new().expect("tempdir");
    // `storages.owner` references the realm's users.
    db.execute("INSERT INTO auth.users (username, password, roles) VALUES ('me', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING")
        .await
        .expect("seed the owner");

    let created = storage::create(
        &db,
        &NewStorage {
            name: name.clone(),
            owner: "me".into(),
            max_file_bytes: Some(1000),
            quota_bytes: 10_000,
            sync_enabled: true,
        },
    )
    .await
    .expect("create");
    assert_eq!((created.used_bytes, created.sync_enabled), (0, true));
    assert!(
        storage::list(&db)
            .await
            .unwrap()
            .iter()
            .any(|s| s.name == name)
    );
    let changed = storage::update(
        &db,
        &name,
        &StorageUpdate {
            max_file_bytes: None,
            quota_bytes: Some(20_000),
            sync_enabled: None,
        },
    )
    .await
    .unwrap()
    .expect("present");
    assert_eq!(
        (changed.quota_bytes, changed.max_file_bytes),
        (20_000, Some(1000))
    );

    // Two paths with the same content share one blob.
    let sha = "ab".repeat(32);
    let blob = dir.path().join("blob");
    for path in ["docs/a.txt", "docs/b.txt"] {
        let staging = dir
            .path()
            .join(format!("staging-{}", path.replace('/', "_")));
        std::fs::write(&staging, b"hello").unwrap();
        let outcome = storage_file::put_file(&db, &name, path, &sha, 5, &staging, &blob)
            .await
            .expect("put");
        assert!(!outcome.existed);
    }
    assert_eq!(
        storage::read(&db, &name).await.unwrap().unwrap().used_bytes,
        10
    );
    assert_eq!(
        storage_file::read_file(&db, &name, "docs/a.txt")
            .await
            .unwrap(),
        Some((sha.clone(), 5))
    );
    assert_eq!(
        storage_file::read_file(&db, &name, "nope").await.unwrap(),
        None
    );
    let listed = storage_file::list_files(&db, &name, "docs/").await.unwrap();
    assert_eq!(
        listed.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        ["docs/a.txt", "docs/b.txt"]
    );
    let page = storage_file::list_files_page(&db, &name, "docs/", Some("docs/a.txt"), 10, false)
        .await
        .unwrap();
    assert_eq!(
        page.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        ["docs/b.txt"]
    );

    let again = dir.path().join("again");
    std::fs::write(&again, b"hello").unwrap();
    assert!(
        storage_file::put_file(&db, &name, "docs/a.txt", &sha, 5, &again, &blob)
            .await
            .unwrap()
            .existed
    );

    let blob_for = |_: &str| blob.clone();
    assert!(
        storage_file::delete_file(&db, &name, "docs/a.txt", blob_for)
            .await
            .unwrap()
    );
    assert!(
        !storage_file::delete_file(&db, &name, "docs/a.txt", blob_for)
            .await
            .unwrap()
    );
    assert!(
        storage_file::delete_file(&db, &name, "docs/b.txt", blob_for)
            .await
            .unwrap()
    );
    assert_eq!(
        storage::read(&db, &name).await.unwrap().unwrap().used_bytes,
        0
    );

    let log = storage_file::sync_log_since(&db, &name, 0).await.unwrap();
    assert!(log.iter().any(|e| e.op == "delete"), "{log:?}");
    assert!(storage::delete(&db, &name).await.unwrap());
    assert!(storage::read(&db, &name).await.unwrap().is_none());
}
