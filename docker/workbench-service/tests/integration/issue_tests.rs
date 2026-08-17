//! Issues, and the `seq` assignment `issue::create` locks around - the one
//! piece of domain logic without a direct analog elsewhere in the estate.

use crate::support::{TEST_USER, database, new_project, skipped};
use workbench_service::domain::issue::{self, IssueUpdate, NewIssue};

fn new_issue(project_id: &str, title: &str) -> NewIssue {
    NewIssue {
        project_id: project_id.to_string(),
        parent_id: None,
        kind: "task".to_string(),
        title: title.to_string(),
        description: None,
        priority: "medium".to_string(),
        assignee: None,
        reporter: TEST_USER.to_string(),
        estimate: None,
    }
}

#[tokio::test]
async fn create_assigns_seq_one_to_the_first_issue() {
    let Some((db, _guard)) = database().await else {
        return skipped("create_assigns_seq_one_to_the_first_issue");
    };

    let project = new_project(&db, "SEQ", "Sequencing").await;
    let created = issue::create(&db, &new_issue(&project.id, "first"))
        .await
        .expect("create");

    assert_eq!(created.seq, 1);
    assert_eq!(created.key(&project.key), "SEQ-1");
}

/// Ten concurrent creates against the same project must produce `seq` values
/// `1..=10` with no gaps or duplicates - the guarantee `pg_advisory_xact_lock`
/// in `issue::create` exists to give. A `MAX(seq) + 1` with no lock at all
/// would race here and either skip a number or hand two issues the same one.
#[tokio::test]
async fn concurrent_creates_in_one_project_get_gapless_unique_seq() {
    let Some((db, _guard)) = database().await else {
        return skipped("concurrent_creates_in_one_project_get_gapless_unique_seq");
    };

    let project = new_project(&db, "RACE", "Racing").await;

    let mut handles = Vec::new();
    for i in 0..10 {
        let db = db.clone();
        let project_id = project.id.clone();
        handles.push(tokio::spawn(async move {
            issue::create(&db, &new_issue(&project_id, &format!("issue {i}")))
                .await
                .expect("create")
        }));
    }

    let mut seqs: Vec<i32> = Vec::new();
    for handle in handles {
        seqs.push(handle.await.expect("join").seq);
    }
    seqs.sort_unstable();

    assert_eq!(
        seqs,
        (1..=10).collect::<Vec<_>>(),
        "seq must be 1..=10 with no gaps or duplicates"
    );
}

/// Two projects racing at once must not interfere with each other's counters -
/// the advisory lock is keyed on the project id specifically, not global.
#[tokio::test]
async fn concurrent_creates_in_different_projects_each_start_at_one() {
    let Some((db, _guard)) = database().await else {
        return skipped("concurrent_creates_in_different_projects_each_start_at_one");
    };

    let project_a = new_project(&db, "PA", "Project A").await;
    let project_b = new_project(&db, "PB", "Project B").await;

    let mut handles = Vec::new();
    for project in [&project_a, &project_b] {
        for i in 0..5 {
            let db = db.clone();
            let project_id = project.id.clone();
            handles.push(tokio::spawn(async move {
                issue::create(&db, &new_issue(&project_id, &format!("issue {i}")))
                    .await
                    .expect("create")
            }));
        }
    }
    for handle in handles {
        handle.await.expect("join");
    }

    let mut seqs_a: Vec<i32> = issue::list_by_project(&db, &project_a.id, None)
        .await
        .expect("list")
        .into_iter()
        .map(|i| i.seq)
        .collect();
    seqs_a.sort_unstable();
    let mut seqs_b: Vec<i32> = issue::list_by_project(&db, &project_b.id, None)
        .await
        .expect("list")
        .into_iter()
        .map(|i| i.seq)
        .collect();
    seqs_b.sort_unstable();

    assert_eq!(seqs_a, (1..=5).collect::<Vec<_>>());
    assert_eq!(seqs_b, (1..=5).collect::<Vec<_>>());
}

#[tokio::test]
async fn transition_changes_status_and_rejects_unknown_status_upstream() {
    let Some((db, _guard)) = database().await else {
        return skipped("transition_changes_status_and_rejects_unknown_status_upstream");
    };

    let project = new_project(&db, "TR", "Transitions").await;
    let created = issue::create(&db, &new_issue(&project.id, "an issue"))
        .await
        .expect("create");
    assert_eq!(created.status, "todo");

    let transitioned = issue::transition(&db, &created.id, "in-progress")
        .await
        .expect("transition")
        .expect("found");
    assert_eq!(transitioned.status, "in-progress");

    // `is_valid_status` is what the API/UI layers check before calling
    // `transition` at all - the domain layer itself does not validate the
    // string, so this documents that the caller's job, not the row's.
    assert!(issue::is_valid_status("done"));
    assert!(!issue::is_valid_status("archived"));
}

#[tokio::test]
async fn update_replaces_editable_fields_but_not_status_or_seq() {
    let Some((db, _guard)) = database().await else {
        return skipped("update_replaces_editable_fields_but_not_status_or_seq");
    };

    let project = new_project(&db, "UPD", "Updates").await;
    let created = issue::create(&db, &new_issue(&project.id, "original title"))
        .await
        .expect("create");

    let updated = issue::update(
        &db,
        &created.id,
        &IssueUpdate {
            title: "new title".to_string(),
            description: Some("now described".to_string()),
            kind: "bug".to_string(),
            priority: "high".to_string(),
            assignee: Some(TEST_USER.to_string()),
            estimate: Some(5),
        },
    )
    .await
    .expect("update")
    .expect("found");

    assert_eq!(updated.title, "new title");
    assert_eq!(updated.kind, "bug");
    assert_eq!(updated.priority, "high");
    assert_eq!(updated.assignee.as_deref(), Some(TEST_USER));
    assert_eq!(updated.estimate, Some(5));
    assert_eq!(updated.seq, created.seq);
    assert_eq!(updated.status, created.status);
}

#[tokio::test]
async fn list_by_project_filters_by_status() {
    let Some((db, _guard)) = database().await else {
        return skipped("list_by_project_filters_by_status");
    };

    let project = new_project(&db, "FLT", "Filtering").await;
    let todo = issue::create(&db, &new_issue(&project.id, "still todo"))
        .await
        .expect("create");
    let done = issue::create(&db, &new_issue(&project.id, "already done"))
        .await
        .expect("create");
    issue::transition(&db, &done.id, "done")
        .await
        .expect("transition");

    let todo_only = issue::list_by_project(&db, &project.id, Some("todo"))
        .await
        .expect("list");
    assert_eq!(todo_only.len(), 1);
    assert_eq!(todo_only[0].id, todo.id);

    let all = issue::list_by_project(&db, &project.id, None)
        .await
        .expect("list");
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn delete_removes_the_issue() {
    let Some((db, _guard)) = database().await else {
        return skipped("delete_removes_the_issue");
    };

    let project = new_project(&db, "DEL", "Deletions").await;
    let created = issue::create(&db, &new_issue(&project.id, "going away"))
        .await
        .expect("create");

    assert!(issue::delete(&db, &created.id).await.expect("delete"));
    assert!(issue::read(&db, &created.id).await.expect("read").is_none());
}
