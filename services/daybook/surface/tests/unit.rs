#![allow(clippy::unwrap_used)]

use daybook_core::{Data, Reminder, Task};
use daybook_host::remote::{Op, Reply};
use daybook_net::{NetError, Store};
use daybook_surface::android::{CHANNEL, pending_ids};
use daybook_surface::complete::{Outcome, complete_with};
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
    assert_eq!(args.get("task_id").and_then(Value::as_str), Some("t"));
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

fn task(id: &str, content: &str) -> Task {
    Task {
        id: id.into(),
        project_id: None,
        content: content.into(),
        description: None,
        is_completed: false,
        sort_order: 0,
        due_at: None,
        due_all_day: false,
        remind_at: None,
        label_ids: vec![],
        recurrence: None,
        created_at: Time::from_unix(1),
        updated_at: Time::from_unix(1),
    }
}

fn shown_by(store: &Store) -> String {
    std::fs::read_to_string(store.dir().join("widget.json")).unwrap_or_default()
}

fn data() -> Data {
    Data {
        tasks: vec![task("a", "Pay the rent"), task("b", "Call the dentist")],
        ..Data::default()
    }
}

fn store(name: &str) -> Store {
    let dir = std::env::temp_dir().join(format!("daybook-complete-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::new(dir);
    store.set_token("refresh");
    store
}

/// A stand-in for the services: `update` answers the change, loading gives `data()`.
fn services(update: Result<Reply, NetError>) -> impl FnMut(Op) -> Result<Reply, NetError> {
    let mut update = Some(update);
    move |op| match op {
        Op::Load => Ok(Reply::Loaded(Box::new(data()))),
        Op::UpdateTask(..) => update.take().expect("one change"),
        _ => panic!("nothing else is asked"),
    }
}

#[quench_test::test]
fn done_from_a_notification_is_sent_and_the_widget_loses_the_task() {
    let store = store("sent");
    let mut finished = task("a", "Pay the rent");
    finished.is_completed = true;
    let outcome = complete_with(&store, "a", services(Ok(Reply::Task(Box::new(finished)))));
    assert_eq!(outcome, Outcome::Completed);
    let shown = shown_by(&store);
    assert!(!shown.contains("Pay the rent"), "{shown}");
    assert!(shown.contains("Call the dentist"), "{shown}");
}

#[quench_test::test]
fn out_of_reach_the_change_waits_in_the_outbox_and_the_widget_shows_it_done() {
    let store = store("queued");
    let outcome = complete_with(
        &store,
        "a",
        services(Err(NetError::Network("no network".into()))),
    );
    assert_eq!(outcome, Outcome::Queued);
    let outbox = store.outbox();
    assert!(
        outbox.contains("update_task") && outbox.contains("\"id\":\"a\""),
        "{outbox}"
    );
    assert!(outbox.contains("\"is_completed\":true"), "{outbox}");
    assert!(!shown_by(&store).contains("Pay the rent"));
}

#[quench_test::test]
fn a_refused_login_signs_out_and_a_task_that_is_gone_is_not_an_error() {
    let store = store("refused");
    let outcome = complete_with(&store, "a", services(Err(NetError::Unauthorized)));
    assert_eq!(outcome, Outcome::SignedOut);
    assert!(store.token().is_none(), "the login is forgotten");

    let store = self::store("gone");
    assert_eq!(
        complete_with(&store, "nope", services(Ok(Reply::Done))),
        Outcome::Gone
    );
    assert!(store.token().is_some());
}

quench_test::main!();
