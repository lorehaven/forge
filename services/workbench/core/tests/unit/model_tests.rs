use quench_serde::json;
use workbench_core::{CreateProject, Issue, RelatedIssues, User};

#[quench_test::test]
fn the_apis_json_reads_back() {
    let text = r#"{"id":"i1","project_id":"p1","parent_id":null,"seq":7,"kind":"task","title":"T","description":null,
        "status":"todo","priority":"low","assignee":null,"reporter":"dev","estimate":null,
        "created_at":"2026-10-29T10:00:00Z","updated_at":"2026-10-29T10:00:00Z"}"#;
    let issue: Issue = json::from_str(text).unwrap();
    assert_eq!(issue.key("WB"), "WB-7");
    assert_eq!(
        json::from_str::<Issue>(&json::to_string(&issue)).unwrap(),
        issue
    );
}

#[quench_test::test]
fn links_and_users() {
    let text = r#"{"blocks":[{"link_id":"l","issue_id":"i","project_key":"WB","seq":2,"title":"B","status":"todo"}],
        "blocked_by":[],"relates_to":[]}"#;
    let related: RelatedIssues = json::from_str(text).unwrap();
    assert_eq!(related.blocks[0].seq, 2);
    let user = User {
        username: "ola".into(),
        display_name: Some(String::new()),
    };
    assert_eq!(user.label(), "ola");
    let user = User {
        username: "ola".into(),
        display_name: Some("Ola N.".into()),
    };
    assert_eq!(user.label(), "Ola N.");
}

#[quench_test::test]
fn a_body_is_the_json_the_api_reads() {
    let body = CreateProject {
        key: "WB".into(),
        name: "Workbench".into(),
        description: None,
    };
    let text = json::to_string(&body);
    assert!(text.contains(r#""key":"WB""#), "{text}");
    assert!(text.contains(r#""description":null"#), "{text}");
}
