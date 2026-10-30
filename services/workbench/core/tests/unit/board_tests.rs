use workbench_core::Issue;
use workbench_core::board::{columns, moved, short_time, status_key};
use workbench_core::model::STATUSES;

fn issue(id: &str, seq: i32, status: &str) -> Issue {
    Issue {
        id: id.into(),
        project_id: "p1".into(),
        parent_id: None,
        seq,
        kind: "task".into(),
        title: format!("Issue {seq}"),
        description: None,
        status: status.into(),
        priority: "medium".into(),
        assignee: None,
        reporter: "dev".into(),
        estimate: None,
        created_at: "2026-10-29T10:00:00Z".into(),
        updated_at: "2026-10-29T10:00:00Z".into(),
    }
}

#[quench_test::test]
fn columns_follow_the_workflow_and_sort_by_number() {
    let issues = [
        issue("c", 3, "todo"),
        issue("a", 1, "done"),
        issue("b", 2, "todo"),
    ];
    let cols = columns(&issues);
    let names: Vec<_> = cols.iter().map(|(s, _)| *s).collect();
    assert_eq!(names, STATUSES);
    let todo: Vec<_> = cols[1].1.iter().map(|i| i.seq).collect();
    assert_eq!(todo, [2, 3]);
    assert_eq!(cols[3].1.len(), 1);
    assert!(cols[0].1.is_empty());
}

#[quench_test::test]
fn moving_a_card_changes_only_its_status() {
    let issues = [issue("a", 1, "todo"), issue("b", 2, "todo")];
    let next = moved(&issues, "a", "in-progress").unwrap();
    assert_eq!(next[0].status, "in-progress");
    assert_eq!(next[1].status, "todo");
    assert_eq!(issues[0].status, "todo");
    assert!(moved(&issues, "a", "todo").is_none());
    assert!(moved(&issues, "zz", "done").is_none());
}

#[quench_test::test]
fn status_keys_and_times() {
    assert_eq!(status_key("in-progress"), "ui_status_in_progress");
    assert_eq!(status_key("todo"), "ui_status_todo");
    assert_eq!(short_time("2026-10-29T10:20:30.5Z"), "2026-10-29 10:20");
    assert_eq!(short_time("2026-10-29"), "2026-10-29");
}
