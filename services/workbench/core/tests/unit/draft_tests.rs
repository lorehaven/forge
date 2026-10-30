use workbench_core::draft::split_issue_key;
use workbench_core::{CommentDraft, DraftError, IssueDraft, LinkDraft, ProjectDraft};

#[quench_test::test]
fn a_project_needs_a_key_and_a_name() {
    let mut draft = ProjectDraft::default();
    assert_eq!(draft.validate().unwrap_err(), DraftError::EmptyFields);
    draft.key = " WB ".into();
    draft.name = "Workbench".into();
    draft.description = "  ".into();
    let body = draft.validate().unwrap();
    assert_eq!((body.key.as_str(), body.name.as_str()), ("WB", "Workbench"));
    assert_eq!(body.description, None);
}

#[quench_test::test]
fn an_issue_form_becomes_a_body() {
    let mut draft = IssueDraft::default();
    assert_eq!(draft.to_create().unwrap_err(), DraftError::TitleRequired);
    draft.title = " Fix it ".into();
    draft.assignee = "ola".into();
    draft.estimate = " 5 ".into();
    let body = draft.to_create().unwrap();
    assert_eq!(body.title, "Fix it");
    assert_eq!(
        (body.kind.as_str(), body.priority.as_str()),
        ("task", "medium")
    );
    assert_eq!(body.assignee.as_deref(), Some("ola"));
    assert_eq!(body.estimate, Some(5));
    assert_eq!(body.description, None);
    assert_eq!(draft.to_update().unwrap().estimate, Some(5));
}

#[quench_test::test]
fn an_estimate_is_a_whole_number_or_nothing() {
    let mut draft = IssueDraft {
        title: "x".into(),
        ..IssueDraft::default()
    };
    for bad in ["-1", "1.5", "many"] {
        draft.estimate = bad.into();
        assert_eq!(
            draft.to_create().unwrap_err(),
            DraftError::InvalidEstimate,
            "{bad}"
        );
    }
    draft.estimate = String::new();
    assert_eq!(draft.to_create().unwrap().estimate, None);
    draft.estimate = "0".into();
    assert_eq!(draft.to_create().unwrap().estimate, Some(0));
}

#[quench_test::test]
fn an_open_issue_fills_the_form() {
    use workbench_core::Issue;
    let issue = Issue {
        id: "i".into(),
        project_id: "p".into(),
        parent_id: None,
        seq: 4,
        kind: "bug".into(),
        title: "Crash".into(),
        description: Some("on start".into()),
        status: "blocked".into(),
        priority: "high".into(),
        assignee: Some("ola".into()),
        reporter: "dev".into(),
        estimate: Some(3),
        created_at: String::new(),
        updated_at: String::new(),
    };
    let draft = IssueDraft::from_issue(&issue);
    assert_eq!(draft.estimate, "3");
    assert_eq!(draft.status, "blocked");
    assert_eq!(
        draft.to_update().unwrap().description.as_deref(),
        Some("on start")
    );
}

#[quench_test::test]
fn comments_and_links() {
    assert_eq!(
        CommentDraft { body: " \n".into() }.validate().unwrap_err(),
        DraftError::CommentRequired
    );
    assert_eq!(
        CommentDraft {
            body: " hi ".into()
        }
        .validate()
        .unwrap()
        .body,
        "hi"
    );
    let link = LinkDraft {
        key: " WB - 4".into(),
        kind: "blocks".into(),
    };
    assert_eq!(link.target().unwrap(), "WB-4");
    for bad in ["", "WB", "WB-x", "-4"] {
        let link = LinkDraft {
            key: bad.into(),
            kind: "blocks".into(),
        };
        assert_eq!(
            link.target().unwrap_err(),
            DraftError::InvalidIssueKey,
            "{bad}"
        );
    }
    assert_eq!(split_issue_key("A-B-12"), Some(("A-B".into(), 12)));
}
