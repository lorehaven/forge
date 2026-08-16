//! Project CRUD, against a real Postgres.

use crate::support::{database, new_project, skipped};
use workbench_service::domain::project::{self, ProjectUpdate};

#[tokio::test]
async fn create_and_read_round_trip() {
    let Some((db, _guard)) = database().await else {
        return skipped("create_and_read_round_trip");
    };

    let created = new_project(&db, "WB", "Workbench").await;
    let read = project::read(&db, &created.id)
        .await
        .expect("read")
        .expect("found");

    assert_eq!(read.id, created.id);
    assert_eq!(read.key, "WB");
    assert_eq!(read.name, "Workbench");
    assert!(read.description.is_none());
}

#[tokio::test]
async fn a_second_project_with_the_same_key_is_rejected() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_second_project_with_the_same_key_is_rejected");
    };

    new_project(&db, "DUP", "First").await;
    let result = project::create(
        &db,
        &project::NewProject {
            key: "DUP".to_string(),
            name: "Second".to_string(),
            description: None,
        },
    )
    .await;

    assert!(result.is_err(), "duplicate key must be rejected");
}

#[tokio::test]
async fn update_replaces_name_and_description() {
    let Some((db, _guard)) = database().await else {
        return skipped("update_replaces_name_and_description");
    };

    let created = new_project(&db, "UPD", "Original name").await;
    let updated = project::update(
        &db,
        &created.id,
        &ProjectUpdate {
            name: "New name".to_string(),
            description: Some("now it has one".to_string()),
        },
    )
    .await
    .expect("update")
    .expect("found");

    assert_eq!(updated.name, "New name");
    assert_eq!(updated.description.as_deref(), Some("now it has one"));
}

#[tokio::test]
async fn delete_removes_the_project() {
    let Some((db, _guard)) = database().await else {
        return skipped("delete_removes_the_project");
    };

    let created = new_project(&db, "DEL", "Going away").await;
    assert!(project::delete(&db, &created.id).await.expect("delete"));
    assert!(
        project::read(&db, &created.id)
            .await
            .expect("read")
            .is_none()
    );
}

#[tokio::test]
async fn list_orders_by_key() {
    let Some((db, _guard)) = database().await else {
        return skipped("list_orders_by_key");
    };

    new_project(&db, "ZZZ", "Last alphabetically").await;
    new_project(&db, "AAA", "First alphabetically").await;

    let keys: Vec<String> = project::list(&db)
        .await
        .expect("list")
        .into_iter()
        .map(|p| p.key)
        .collect();

    let aaa_pos = keys.iter().position(|k| k == "AAA");
    let zzz_pos = keys.iter().position(|k| k == "ZZZ");
    assert!(aaa_pos.is_some() && zzz_pos.is_some());
    assert!(aaa_pos < zzz_pos, "AAA must sort before ZZZ");
}
