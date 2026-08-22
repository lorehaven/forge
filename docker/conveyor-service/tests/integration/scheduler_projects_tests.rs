//! `scheduler::projects` - conveyor's organisational tree. Raw SQL against a
//! real Postgres (recursive CTEs), so these need `CONVEYOR_TEST_DATABASE_URL`
//! - see `support.rs`.

use crate::support::{database, skipped};
use conveyor_service::domain::Provider;
use conveyor_service::scheduler::projects::{self, DeleteOutcome, MoveOutcome, NewProject};
use conveyor_service::scheduler::repos::{self, NewRepo};

async fn make_project(db: &quench_db::prelude::Db, name: &str, parent_id: Option<&str>) -> String {
    projects::create(
        db,
        &NewProject {
            name: name.to_string(),
            parent_id: parent_id.map(str::to_string),
        },
    )
    .await
    .expect("create project")
    .id
}

#[tokio::test]
async fn create_and_read_round_trip() {
    let Some((db, _guard)) = database().await else {
        return skipped("create_and_read_round_trip");
    };

    let id = make_project(&db, "root-a", None).await;
    let read = projects::read(&db, &id)
        .await
        .expect("read")
        .expect("present");
    assert_eq!(read.name, "root-a");
    assert!(read.parent_id.is_none());
}

#[tokio::test]
async fn read_missing_project_is_none() {
    let Some((db, _guard)) = database().await else {
        return skipped("read_missing_project_is_none");
    };

    assert!(
        projects::read(&db, "00000000-0000-0000-0000-000000000000")
            .await
            .expect("read")
            .is_none()
    );
}

#[tokio::test]
async fn list_children_separates_roots_from_nested_nodes() {
    let Some((db, _guard)) = database().await else {
        return skipped("list_children_separates_roots_from_nested_nodes");
    };

    let root = make_project(&db, "list-root", None).await;
    let _child_a = make_project(&db, "list-child-a", Some(&root)).await;
    let _child_b = make_project(&db, "list-child-b", Some(&root)).await;

    let roots = projects::list_children(&db, None).await.expect("roots");
    assert!(roots.iter().any(|p| p.id == root));

    let children = projects::list_children(&db, Some(&root))
        .await
        .expect("children");
    assert_eq!(children.len(), 2);
    // Ordered by name.
    assert_eq!(children[0].name, "list-child-a");
    assert_eq!(children[1].name, "list-child-b");
}

#[tokio::test]
async fn list_all_includes_every_registered_project() {
    let Some((db, _guard)) = database().await else {
        return skipped("list_all_includes_every_registered_project");
    };

    let a = make_project(&db, "list-all-a", None).await;
    let b = make_project(&db, "list-all-b", Some(&a)).await;

    let all = projects::list_all(&db).await.expect("list all");
    let ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
    assert!(ids.contains(&a.as_str()));
    assert!(ids.contains(&b.as_str()));
}

#[tokio::test]
async fn rename_changes_the_name_and_is_none_for_a_missing_project() {
    let Some((db, _guard)) = database().await else {
        return skipped("rename_changes_the_name_and_is_none_for_a_missing_project");
    };

    let id = make_project(&db, "before-rename", None).await;
    let renamed = projects::rename(&db, &id, "after-rename")
        .await
        .expect("rename")
        .expect("present");
    assert_eq!(renamed.name, "after-rename");

    assert!(
        projects::rename(&db, "00000000-0000-0000-0000-000000000000", "x")
            .await
            .expect("rename missing")
            .is_none()
    );
}

#[tokio::test]
async fn move_to_relocates_a_project_under_a_new_parent() {
    let Some((db, _guard)) = database().await else {
        return skipped("move_to_relocates_a_project_under_a_new_parent");
    };

    let parent_a = make_project(&db, "move-parent-a", None).await;
    let parent_b = make_project(&db, "move-parent-b", None).await;
    let child = make_project(&db, "move-child", Some(&parent_a)).await;

    match projects::move_to(&db, &child, Some(&parent_b))
        .await
        .expect("move")
    {
        MoveOutcome::Moved(project) => {
            assert_eq!(project.parent_id.as_deref(), Some(parent_b.as_str()))
        }
        _ => panic!("expected Moved"),
    }
}

#[tokio::test]
async fn move_to_rejects_moving_a_project_under_itself() {
    let Some((db, _guard)) = database().await else {
        return skipped("move_to_rejects_moving_a_project_under_itself");
    };

    let id = make_project(&db, "move-self", None).await;
    assert!(matches!(
        projects::move_to(&db, &id, Some(&id)).await.expect("move"),
        MoveOutcome::WouldCycle
    ));
}

#[tokio::test]
async fn move_to_rejects_moving_a_project_under_its_own_descendant() {
    let Some((db, _guard)) = database().await else {
        return skipped("move_to_rejects_moving_a_project_under_its_own_descendant");
    };

    let root = make_project(&db, "move-cycle-root", None).await;
    let child = make_project(&db, "move-cycle-child", Some(&root)).await;
    let grandchild = make_project(&db, "move-cycle-grandchild", Some(&child)).await;

    assert!(matches!(
        projects::move_to(&db, &root, Some(&grandchild))
            .await
            .expect("move"),
        MoveOutcome::WouldCycle
    ));
}

#[tokio::test]
async fn move_to_is_not_found_for_a_missing_project() {
    let Some((db, _guard)) = database().await else {
        return skipped("move_to_is_not_found_for_a_missing_project");
    };

    assert!(matches!(
        projects::move_to(&db, "00000000-0000-0000-0000-000000000000", None)
            .await
            .expect("move"),
        MoveOutcome::NotFound
    ));
}

#[tokio::test]
async fn delete_refuses_a_project_with_children() {
    let Some((db, _guard)) = database().await else {
        return skipped("delete_refuses_a_project_with_children");
    };

    let root = make_project(&db, "delete-parent", None).await;
    let _child = make_project(&db, "delete-child", Some(&root)).await;

    assert!(matches!(
        projects::delete(&db, &root).await.expect("delete"),
        DeleteOutcome::HasChildren
    ));
}

#[tokio::test]
async fn delete_refuses_a_project_with_an_attached_repo() {
    let Some((db, _guard)) = database().await else {
        return skipped("delete_refuses_a_project_with_an_attached_repo");
    };

    let project = make_project(&db, "delete-with-repo", None).await;
    repos::create(
        &db,
        &NewRepo {
            provider: Provider::Generic,
            owner: "tests".to_string(),
            name: "delete-with-repo-repo".to_string(),
            clone_url: "file:///tmp/delete-with-repo".to_string(),
            default_branch: "master".to_string(),
            registered_by: crate::support::TEST_USER.to_string(),
            project_id: project.clone(),
        },
    )
    .await
    .expect("create repo");

    assert!(matches!(
        projects::delete(&db, &project).await.expect("delete"),
        DeleteOutcome::HasRepo
    ));
}

#[tokio::test]
async fn delete_succeeds_for_a_leaf_project_and_is_not_found_the_second_time() {
    let Some((db, _guard)) = database().await else {
        return skipped("delete_succeeds_for_a_leaf_project_and_is_not_found_the_second_time");
    };

    let id = make_project(&db, "delete-leaf", None).await;
    assert!(matches!(
        projects::delete(&db, &id).await.expect("delete"),
        DeleteOutcome::Deleted
    ));
    assert!(matches!(
        projects::delete(&db, &id).await.expect("delete again"),
        DeleteOutcome::NotFound
    ));
}

#[tokio::test]
async fn ancestor_chain_includes_every_node_up_to_the_root() {
    let Some((db, _guard)) = database().await else {
        return skipped("ancestor_chain_includes_every_node_up_to_the_root");
    };

    let root = make_project(&db, "chain-root", None).await;
    let mid = make_project(&db, "chain-mid", Some(&root)).await;
    let leaf = make_project(&db, "chain-leaf", Some(&mid)).await;

    let chain = projects::ancestor_chain(&db, &leaf).await.expect("chain");
    assert!(chain.contains(&leaf));
    assert!(chain.contains(&mid));
    assert!(chain.contains(&root));
    assert_eq!(chain.len(), 3);
}

#[tokio::test]
async fn descendant_ids_includes_the_roots_and_everything_nested() {
    let Some((db, _guard)) = database().await else {
        return skipped("descendant_ids_includes_the_roots_and_everything_nested");
    };

    let root = make_project(&db, "descendant-root", None).await;
    let child = make_project(&db, "descendant-child", Some(&root)).await;
    let grandchild = make_project(&db, "descendant-grandchild", Some(&child)).await;
    let unrelated = make_project(&db, "descendant-unrelated", None).await;

    let ids = projects::descendant_ids(&db, std::slice::from_ref(&root))
        .await
        .expect("descendants");
    assert!(ids.contains(&root));
    assert!(ids.contains(&child));
    assert!(ids.contains(&grandchild));
    assert!(!ids.contains(&unrelated));
}

#[tokio::test]
async fn descendant_ids_is_empty_for_an_empty_root_list() {
    let Some((db, _guard)) = database().await else {
        return skipped("descendant_ids_is_empty_for_an_empty_root_list");
    };

    assert!(
        projects::descendant_ids(&db, &[])
            .await
            .expect("descendants")
            .is_empty()
    );
}

#[tokio::test]
async fn full_path_joins_names_from_root_to_leaf() {
    let Some((db, _guard)) = database().await else {
        return skipped("full_path_joins_names_from_root_to_leaf");
    };

    let root = make_project(&db, "path-root", None).await;
    let mid = make_project(&db, "path-mid", Some(&root)).await;
    let leaf = make_project(&db, "path-leaf", Some(&mid)).await;

    let path = projects::full_path(&db, &leaf)
        .await
        .expect("path")
        .expect("present");
    assert_eq!(path, "path-root/path-mid/path-leaf");
}

#[tokio::test]
async fn full_path_is_none_for_a_missing_project() {
    let Some((db, _guard)) = database().await else {
        return skipped("full_path_is_none_for_a_missing_project");
    };

    assert!(
        projects::full_path(&db, "00000000-0000-0000-0000-000000000000")
            .await
            .expect("path")
            .is_none()
    );
}
