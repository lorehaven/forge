//! Comments and labels, against a real Postgres.

use crate::support::{TEST_USER, database, new_project, skipped};
use workbench_service::domain::comment::{self, NewComment};
use workbench_service::domain::issue::{self, NewIssue};
use workbench_service::domain::label::{self, NewLabel};

async fn new_issue_in(db: &quench_db::prelude::Db, project_id: &str) -> issue::Issue {
    issue::create(
        db,
        &NewIssue {
            project_id: project_id.to_string(),
            parent_id: None,
            kind: "task".to_string(),
            title: "an issue".to_string(),
            description: None,
            priority: "medium".to_string(),
            assignee: None,
            reporter: TEST_USER.to_string(),
        },
    )
    .await
    .expect("create issue")
}

#[tokio::test]
async fn comments_list_oldest_first_and_can_be_deleted() {
    let Some((db, _guard)) = database().await else {
        return skipped("comments_list_oldest_first_and_can_be_deleted");
    };

    let project = new_project(&db, "CMT", "Comments").await;
    let issue = new_issue_in(&db, &project.id).await;

    let first = comment::create(
        &db,
        &NewComment {
            issue_id: issue.id.clone(),
            author: TEST_USER.to_string(),
            body: "first".to_string(),
        },
    )
    .await
    .expect("create comment");
    let second = comment::create(
        &db,
        &NewComment {
            issue_id: issue.id.clone(),
            author: TEST_USER.to_string(),
            body: "second".to_string(),
        },
    )
    .await
    .expect("create comment");

    let comments = comment::list_by_issue(&db, &issue.id)
        .await
        .expect("list comments");
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].id, first.id);
    assert_eq!(comments[1].id, second.id);

    assert!(comment::delete(&db, &first.id).await.expect("delete"));
    let remaining = comment::list_by_issue(&db, &issue.id)
        .await
        .expect("list comments");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, second.id);
}

#[tokio::test]
async fn attaching_a_label_twice_is_idempotent() {
    let Some((db, _guard)) = database().await else {
        return skipped("attaching_a_label_twice_is_idempotent");
    };

    let project = new_project(&db, "LBL", "Labels").await;
    let issue = new_issue_in(&db, &project.id).await;
    let urgent = label::create(
        &db,
        &NewLabel {
            project_id: project.id.clone(),
            name: "urgent".to_string(),
            color: "#ff0000".to_string(),
        },
    )
    .await
    .expect("create label");

    label::attach(&db, &issue.id, &urgent.id)
        .await
        .expect("attach");
    label::attach(&db, &issue.id, &urgent.id)
        .await
        .expect("attach again");

    let labels = label::list_for_issue(&db, &issue.id)
        .await
        .expect("list labels");
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].id, urgent.id);

    label::detach(&db, &issue.id, &urgent.id)
        .await
        .expect("detach");
    let labels = label::list_for_issue(&db, &issue.id)
        .await
        .expect("list labels");
    assert!(labels.is_empty());
}

#[tokio::test]
async fn a_second_label_with_the_same_name_in_a_project_is_rejected() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_second_label_with_the_same_name_in_a_project_is_rejected");
    };

    let project = new_project(&db, "DUPL", "Duplicate labels").await;
    label::create(
        &db,
        &NewLabel {
            project_id: project.id.clone(),
            name: "urgent".to_string(),
            color: "#ff0000".to_string(),
        },
    )
    .await
    .expect("create label");

    let result = label::create(
        &db,
        &NewLabel {
            project_id: project.id.clone(),
            name: "urgent".to_string(),
            color: "#00ff00".to_string(),
        },
    )
    .await;

    assert!(
        result.is_err(),
        "duplicate label name in one project must be rejected"
    );
}
