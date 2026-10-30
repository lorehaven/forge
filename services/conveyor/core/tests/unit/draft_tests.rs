use conveyor_core::{DraftError, LaunchDraft, RepoDraft};

fn filled() -> RepoDraft {
    RepoDraft {
        owner: " lorehaven ".into(),
        name: "forge".into(),
        clone_url: " https://example.test/forge.git ".into(),
        default_branch: String::new(),
        project_id: "p1".into(),
        provider: "generic".into(),
        enabled: true,
    }
}

#[quench_test::test]
fn a_repository_form_is_checked_and_trimmed() {
    let body = filled().to_register().unwrap();
    assert_eq!(body.owner, "lorehaven");
    assert_eq!(body.clone_url, "https://example.test/forge.git");
    assert_eq!(body.default_branch, None);
    assert_eq!(body.provider.as_deref(), Some("generic"));

    let mut draft = filled();
    draft.owner = "  ".into();
    assert_eq!(draft.to_register(), Err(DraftError::OwnerNameEmpty));
    let mut draft = filled();
    draft.project_id = String::new();
    assert_eq!(draft.to_register(), Err(DraftError::ProjectRequired));
    let mut draft = filled();
    draft.clone_url = " ".into();
    assert_eq!(draft.to_update(), Err(DraftError::BadCloneUrl));
}

#[quench_test::test]
fn an_edit_sends_every_field() {
    let mut draft = filled();
    draft.default_branch = "main".into();
    draft.enabled = false;
    let body = draft.to_update().unwrap();
    assert_eq!(body.name.as_deref(), Some("forge"));
    assert_eq!(body.default_branch.as_deref(), Some("main"));
    assert_eq!(body.enabled, Some(false));
    assert_eq!(body.project_id.as_deref(), Some("p1"));
}

#[quench_test::test]
fn error_keys_are_the_old_pages_keys() {
    assert_eq!(DraftError::NoStage.key(), "ui_launch_err_none");
    assert_eq!(DraftError::BadCloneUrl.key(), "ui_repos_err_bad_clone_url");
}

#[quench_test::test]
fn ticked_boxes_and_typed_extras_are_combined_without_repeats() {
    let draft = LaunchDraft {
        git_ref: String::new(),
        ticked: vec!["check".into(), "security".into()],
        extra: " build , check,, lint ".into(),
    };
    assert_eq!(draft.stages(), ["check", "security", "build", "lint"]);
    let none = LaunchDraft {
        extra: " , ".into(),
        ..LaunchDraft::default()
    };
    assert!(none.stages().is_empty());
    assert_eq!(none.to_trigger(), Err(DraftError::NoStage));
}

#[quench_test::test]
fn a_bare_branch_becomes_a_full_ref() {
    let mut draft = LaunchDraft {
        ticked: vec!["check".into()],
        ..LaunchDraft::default()
    };
    assert_eq!(draft.full_ref(), None);
    draft.git_ref = " main ".into();
    assert_eq!(draft.full_ref().as_deref(), Some("refs/heads/main"));
    draft.git_ref = "refs/tags/v1".into();
    let body = draft.to_trigger().unwrap();
    assert_eq!(body.git_ref.as_deref(), Some("refs/tags/v1"));
    assert_eq!(body.stages, Some(vec!["check".to_string()]));
}
