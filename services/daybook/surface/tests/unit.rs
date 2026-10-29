#![allow(clippy::unwrap_used)]

use daybook_core::Reminder;
use daybook_surface::android::{CHANNEL, pending_ids};
use daybook_surface::reminder_args;
use quench_android_surface::Value;
use quench_core::time::Time;

#[quench_test::test]
fn a_reminder_is_what_the_notifications_plugin_schedules() {
    let r = Reminder {
        id: 42,
        task_id: "t".into(),
        at: Time::from_unix(1_800_000_000),
        title: "Call".into(),
        body: "Due Fri 30 Oct 08:30".into(),
    };
    let args = reminder_args(&r);
    assert_eq!(args.get("id").and_then(Value::as_i64), Some(42));
    assert_eq!(
        args.get("at_ms").and_then(Value::as_i64),
        Some(1_800_000_000_000)
    );
    assert_eq!(args.get("title").and_then(Value::as_str), Some("Call"));
    assert_eq!(
        args.get("body").and_then(Value::as_str),
        Some("Due Fri 30 Oct 08:30")
    );
    assert_eq!(args.get("channel").and_then(Value::as_str), Some(CHANNEL));
}

#[quench_test::test]
fn the_armed_reminders_are_read_from_the_plugins_list() {
    let list = quench_serde::json::from_str::<Value>(
        r#"[{"id":1,"at_ms":5},{"id":-7,"at_ms":6},{"nope":true}]"#,
    )
    .unwrap();
    assert_eq!(pending_ids(&list), [1, -7]);
    assert!(pending_ids(&Value::Null).is_empty());
}

#[quench_test::test]
fn a_launch_file_says_what_to_show() {
    use daybook_core::{ListRef, Msg};
    use daybook_surface::launch::parse;
    assert_eq!(
        parse("add\np:home\n"),
        Some(Msg::Show {
            list: ListRef::Project("home".into()),
            add: true,
            task: None
        })
    );
    assert_eq!(
        parse("open\nl:x\nt1\n"),
        Some(Msg::Show {
            list: ListRef::Label("x".into()),
            add: false,
            task: Some("t1".into())
        })
    );
    assert_eq!(
        parse("add\n"),
        Some(Msg::Show {
            list: ListRef::Inbox,
            add: true,
            task: None
        })
    );
    assert_eq!(
        parse("add\njunk"),
        Some(Msg::Show {
            list: ListRef::Inbox,
            add: true,
            task: None
        })
    );
    assert_eq!(parse("dance\ninbox"), None);
    assert_eq!(parse(""), None);
}

#[quench_test::test]
fn a_widget_refresh_with_nobody_signed_in_empties_the_widget() {
    use daybook_net::Store;
    use daybook_surface::widget::{Outcome, refresh};
    let dir = std::env::temp_dir().join(format!("daybook-widget-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Store::new(&dir);
    store.set_widget_snapshot("{}");
    assert!(dir.join("widget.json").exists());
    assert_eq!(refresh(&store), Outcome::SignedOut);
    assert!(!dir.join("widget.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[quench_test::test]
fn a_launch_is_read_once() {
    use daybook_net::Store;
    let dir = std::env::temp_dir().join(format!("daybook-launch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("launch"), "add\ninbox\n").unwrap();
    let store = Store::new(&dir);
    assert_eq!(store.take_launch().as_deref(), Some("add\ninbox\n"));
    assert_eq!(store.take_launch(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

quench_test::main!();
