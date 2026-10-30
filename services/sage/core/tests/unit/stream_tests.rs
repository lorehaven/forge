use sage_core::stream::EVENT_NAMES;
use sage_core::{Done, Status, StreamEvent};

fn all() -> Vec<StreamEvent> {
    vec![
        StreamEvent::Status(Status {
            stage: "searching".into(),
            detail: Some("rust · wasm".into()),
        }),
        StreamEvent::Delta("two\nlines \"quoted\" <b>".into()),
        StreamEvent::Replace("whole".into()),
        StreamEvent::Done(Done {
            conversation_id: "c".into(),
            user_message_id: Some("u".into()),
            message_id: "m".into(),
            title: "t".into(),
        }),
        StreamEvent::Failure("it broke".into()),
    ]
}

#[quench_test::test]
fn every_event_reads_back_as_it_was_sent() {
    for event in all() {
        let parsed = StreamEvent::parse(event.name(), &event.data());
        assert_eq!(parsed, Some(event));
    }
}

#[quench_test::test]
fn a_frame_is_one_event_with_one_data_line() {
    for event in all() {
        let frame = event.frame();
        assert!(frame.starts_with(&format!("event: {}\ndata: ", event.name())));
        assert!(frame.ends_with("\n\n"));
        // Line breaks inside the text are escaped, so the data stays on one line.
        assert_eq!(frame.matches('\n').count(), 3, "{frame}");
    }
}

#[quench_test::test]
fn the_names_the_browser_listens_for_are_all_events() {
    let names: Vec<&str> = all().iter().map(StreamEvent::name).collect();
    for name in EVENT_NAMES {
        assert!(names.contains(&name), "{name}");
    }
    assert!(!EVENT_NAMES.contains(&"error"));
}

#[quench_test::test]
fn strangers_are_ignored() {
    assert_eq!(StreamEvent::parse("other", "{}"), None);
    assert_eq!(StreamEvent::parse("delta", "not json"), None);
    assert_eq!(StreamEvent::parse("done", "{}"), None);
}
