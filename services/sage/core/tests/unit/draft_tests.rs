use sage_core::draft::{PROJECT_NAME_MAX, upload_result};
use sage_core::{DraftError, MessageDraft, ProjectDraft, preview};

fn draft() -> MessageDraft {
    MessageDraft {
        text: "  hello  ".into(),
        conversation_id: "c1".into(),
        project_id: Some("p1".into()),
        instance_id: "i1".into(),
        search_provider: "duckduckgo".into(),
        file_ids: vec!["f1".into(), "f2".into()],
    }
}

#[quench_test::test]
fn a_message_becomes_the_body_of_a_turn() {
    let body = draft().body().unwrap();
    assert_eq!(body.message, "hello");
    assert_eq!(body.conversation_id, "c1");
    assert_eq!(body.project_id.as_deref(), Some("p1"));
    assert_eq!(body.instance_id, "i1");
    assert_eq!(body.search_provider.as_deref(), Some("duckduckgo"));
    assert_eq!(body.file_ids, "f1,f2");
}

#[quench_test::test]
fn nothing_to_say_or_no_model_is_refused() {
    let mut empty = draft();
    empty.text = "  \n ".into();
    assert_eq!(empty.body(), Err(DraftError("ui_chat_error_empty")));
    let mut no_model = draft();
    no_model.instance_id.clear();
    assert_eq!(
        no_model.body(),
        Err(DraftError("ui_chat_no_model_available"))
    );
}

#[quench_test::test]
fn no_provider_or_files_send_none() {
    let mut d = draft();
    d.search_provider.clear();
    d.file_ids.clear();
    let body = d.body().unwrap();
    assert_eq!(body.search_provider, None);
    assert_eq!(body.file_ids, "");
}

#[quench_test::test]
fn a_project_needs_a_name_of_reasonable_length() {
    let ok = ProjectDraft {
        name: " Notes ".into(),
    };
    assert_eq!(ok.body().unwrap().name, "Notes");
    assert!(ProjectDraft { name: "  ".into() }.body().is_err());
    assert!(
        ProjectDraft {
            name: "x".repeat(PROJECT_NAME_MAX + 1)
        }
        .body()
        .is_err()
    );
}

#[quench_test::test]
fn the_preview_keeps_thirty_characters() {
    assert_eq!(preview("short"), "short");
    assert_eq!(preview(&"a".repeat(30)), "a".repeat(30));
    assert_eq!(preview(&"b".repeat(31)), format!("{}...", "b".repeat(30)));
    assert_eq!(preview("  padded  "), "padded");
    // Characters, not bytes.
    assert_eq!(preview(&"ż".repeat(40)), format!("{}...", "ż".repeat(30)));
}

#[quench_test::test]
fn an_upload_report_is_read() {
    let raw = r#"{"name":"a.txt","status":201,"body":"{\"id\":\"f\"}"}"#;
    assert_eq!(
        upload_result(raw),
        Some(("a.txt".into(), 201, "{\"id\":\"f\"}".into()))
    );
    assert_eq!(upload_result("nonsense"), None);
}

#[quench_test::test]
fn sizes_are_shown_in_the_unit_that_reads_best() {
    use sage_core::human_bytes;
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(512), "512 B");
    assert_eq!(human_bytes(1536), "1.5 KB");
    assert_eq!(human_bytes(3 * 1024 * 1024), "3.0 MB");
    assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
    assert_eq!(human_bytes(-4), "0 B");
}
