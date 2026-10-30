use sage_core::{ConversationView, FileInfo, MessageNode, ModelsView, Source};

fn node(id: &str, siblings: &[&str]) -> MessageNode {
    MessageNode {
        id: id.into(),
        parent_id: None,
        role: "assistant".into(),
        content: "x".into(),
        created_at: "2026-10-29T10:00:00Z".into(),
        siblings: siblings.iter().map(ToString::to_string).collect(),
        sources: Vec::new(),
        attachments: Vec::new(),
    }
}

#[quench_test::test]
fn alternatives_are_counted_and_wrap_around() {
    let n = node("b", &["a", "b", "c"]);
    assert_eq!(n.position(), (2, 3));
    assert_eq!(n.neighbours(), Some(("a", "c")));
    let first = node("a", &["a", "b", "c"]);
    assert_eq!(first.neighbours(), Some(("c", "b")));
    let last = node("c", &["a", "b", "c"]);
    assert_eq!(last.neighbours(), Some(("b", "a")));
}

#[quench_test::test]
fn a_message_alone_has_no_neighbours() {
    let n = node("a", &["a"]);
    assert_eq!(n.position(), (1, 1));
    assert_eq!(n.neighbours(), None);
    assert_eq!(node("a", &[]).position(), (1, 1));
}

#[quench_test::test]
fn the_service_json_reads_with_extra_fields_and_missing_options() {
    let json = r#"{"id":"f","owner":"me","file_name":"a.png","mime_type":"image/png","file_size":3,"conversation_id":"c",
        "project_id":null,"message_id":null,"status":"uploaded","error_message":null,"created_at":"x","updated_at":"y"}"#;
    let file: FileInfo = quench_serde::json::from_str(json).unwrap();
    assert!(file.is_image());
    assert!(file.in_progress());
    let short =
        r#"{"id":"f","file_name":"a.txt","mime_type":"text/plain","file_size":3,"status":"ready"}"#;
    let file: FileInfo = quench_serde::json::from_str(short).unwrap();
    assert!(!file.in_progress());
    assert_eq!(file.conversation_id, None);
}

#[quench_test::test]
fn a_conversation_round_trips() {
    let view = ConversationView {
        id: "c".into(),
        title: "t".into(),
        project_id: Some("p".into()),
        messages: vec![MessageNode {
            sources: vec![Source {
                file_name: "[1] example.com".into(),
                chunk_index: None,
                detail: Some("https://example.com".into()),
                similarity: Some(0.5),
            }],
            ..node("m", &["m"])
        }],
    };
    let text = quench_serde::json::to_string(&view);
    assert_eq!(
        quench_serde::json::from_str::<ConversationView>(&text).unwrap(),
        view
    );
}

#[quench_test::test]
fn models_read_back() {
    let view = ModelsView {
        switchboard_ok: true,
        ready: true,
        instances: Vec::new(),
        defaults: Vec::new(),
        search_providers: Vec::new(),
        default_search_provider: "duckduckgo".into(),
        accept: ".txt".into(),
    };
    let text = quench_serde::json::to_string(&view);
    assert_eq!(
        quench_serde::json::from_str::<ModelsView>(&text).unwrap(),
        view
    );
}
