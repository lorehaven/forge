use daybook_core::calendar::{all_day_time, at_local, format_clock, iso_weekday, urgency};
use daybook_core::picker::{month_grid, month_title, shift_month};
use daybook_core::{
    Draft, Due, ListRef, Preset, Recurrence, Remind, Task, Unit, Urgency, format_due, is_overdue,
    notification_id, open_count, order_tasks, pending_reminders,
};
use quench_core::date::Date;
use quench_core::time::Time;
use quench_serde::Value;
use quench_tz::Zone;

fn warsaw() -> Zone {
    Zone::named("Europe/Warsaw").expect("zone")
}

fn date(y: i32, m: u32, d: u32) -> Date {
    Date::from_ymd(y, m, d).expect("date")
}

fn at(zone: &Zone, y: i32, m: u32, d: u32, h: u32, min: u32) -> Time {
    at_local(date(y, m, d), h, min, zone).expect("time")
}

fn task(id: &str) -> Task {
    Task {
        id: id.into(),
        project_id: None,
        content: id.into(),
        description: None,
        is_completed: false,
        sort_order: 0,
        due_at: None,
        due_all_day: false,
        remind_at: None,
        label_ids: Vec::new(),
        recurrence: None,
        created_at: Time::from_unix(1),
        updated_at: Time::from_unix(1),
    }
}

fn json(text: &str) -> Value {
    quench_serde::json::from_str(text).expect("json")
}

#[quench_test::test]
fn a_task_is_read_from_the_services_json() {
    let t = Task::from_value(&json(
        r#"{"id":"t1","project_id":null,"content":"Pay rent","description":"flat","is_completed":false,"sort_order":3,
        "due_at":"2026-10-28T12:30:00Z","due_all_day":false,"remind_at":"2026-10-28T12:00:00Z","label_ids":["a","b"],
        "recurrence":{"unit":"week","interval":2,"weekdays":[1,4],"mode":"fixed","timezone":"Europe/Warsaw"},
        "created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-02T00:00:00Z"}"#,
    ))
    .expect("task");
    assert_eq!(t.content, "Pay rent");
    assert_eq!(t.project_id, None);
    assert_eq!(t.label_ids, ["a", "b"]);
    assert_eq!(
        t.due_at,
        Some(Time::parse_rfc3339("2026-10-28T12:30:00Z").unwrap())
    );
    assert_eq!(
        t.recurrence.expect("rule").describe(),
        "Every 2 weeks on Mon, Thu"
    );
}

#[quench_test::test]
fn a_replacing_update_sends_every_field() {
    let mut t = task("x");
    t.due_at = Some(Time::from_unix(1_800_000_000));
    let body = t.to_update();
    for key in [
        "content",
        "description",
        "project_id",
        "is_completed",
        "due_at",
        "remind_at",
        "label_ids",
        "recurrence",
    ] {
        assert!(body.get(key).is_some(), "{key} missing");
    }
    assert!(body.get("remind_at").unwrap().is_null());
}

#[quench_test::test]
fn repeats_are_described() {
    let z = "UTC";
    assert_eq!(Recurrence::every(Unit::Day, z).describe(), "Every day");
    let mut r = Recurrence::every(Unit::Day, z);
    r.interval = 3;
    r.mode = daybook_core::Mode::AfterCompletion;
    assert_eq!(r.describe(), "3 days after completion");
}

#[quench_test::test]
fn weekdays_are_iso() {
    assert_eq!(iso_weekday(date(2026, 10, 26)), 1);
    assert_eq!(iso_weekday(date(2026, 11, 1)), 7);
    assert_eq!(iso_weekday(date(1970, 1, 1)), 4);
}

#[quench_test::test]
fn an_all_day_date_is_that_utc_date_in_any_zone() {
    let due = all_day_time(date(2026, 10, 28));
    for name in ["Pacific/Auckland", "America/Los_Angeles", "Europe/Warsaw"] {
        let z = Zone::named(name).unwrap();
        assert_eq!(daybook_core::due_day(due, true, &z), date(2026, 10, 28));
    }
}

#[quench_test::test]
fn urgency_follows_the_distance() {
    let z = warsaw();
    let now = at(&z, 2026, 10, 28, 10, 0);
    let all_day = |d: u32| all_day_time(date(2026, 10, d));
    assert_eq!(urgency(all_day(27), true, now, &z), Urgency::Overdue);
    assert_eq!(urgency(all_day(28), true, now, &z), Urgency::Today);
    assert_eq!(urgency(all_day(29), true, now, &z), Urgency::Tomorrow);
    assert_eq!(urgency(all_day(31), true, now, &z), Urgency::Soon);
    assert_eq!(urgency(all_day(30), true, now, &z), Urgency::Soon);
    assert_eq!(
        urgency(all_day_time(date(2026, 11, 4)), true, now, &z),
        Urgency::Week
    );
    assert_eq!(
        urgency(all_day_time(date(2026, 11, 5)), true, now, &z),
        Urgency::Later
    );
    assert_eq!(Urgency::Later.rgb(), None);
    assert_eq!(urgency(all_day(20), true, now, &z), Urgency::Overdue);
    assert_eq!(
        urgency(all_day_time(date(2026, 12, 1)), true, now, &z),
        Urgency::Later
    );
}

#[quench_test::test]
fn a_timed_task_is_late_once_its_time_has_passed() {
    let z = warsaw();
    let now = at(&z, 2026, 10, 28, 10, 0);
    assert!(is_overdue(at(&z, 2026, 10, 28, 9, 59), false, now, &z));
    assert!(!is_overdue(at(&z, 2026, 10, 28, 10, 1), false, now, &z));
    assert!(!is_overdue(all_day_time(date(2026, 10, 28)), true, now, &z));
}

#[quench_test::test]
fn due_dates_are_written_for_people() {
    let z = warsaw();
    let now = at(&z, 2026, 10, 28, 10, 0);
    assert_eq!(
        format_due(at(&z, 2026, 10, 28, 14, 30), false, now, &z),
        "Today 14:30"
    );
    assert_eq!(
        format_due(all_day_time(date(2026, 10, 29)), true, now, &z),
        "Tomorrow"
    );
    assert_eq!(
        format_due(all_day_time(date(2026, 10, 27)), true, now, &z),
        "Yesterday"
    );
    assert_eq!(
        format_due(at(&z, 2026, 11, 2, 9, 5), false, now, &z),
        "Mon 2 Nov 09:05"
    );
    assert_eq!(
        format_due(all_day_time(date(2027, 3, 1)), true, now, &z),
        "Mon 1 Mar 2027"
    );
    assert_eq!(format_clock(7, 3), "07:03");
}

#[quench_test::test]
fn open_tasks_come_by_due_date_all_day_first() {
    let z = warsaw();
    let mut timed = task("timed");
    timed.due_at = Some(at(&z, 2026, 10, 28, 8, 0));
    let mut all_day = task("allday");
    all_day.due_at = Some(all_day_time(date(2026, 10, 28)));
    all_day.due_all_day = true;
    let mut later = task("later");
    later.due_at = Some(at(&z, 2026, 10, 30, 8, 0));
    let mut undated_b = task("b");
    undated_b.sort_order = 2;
    let mut undated_a = task("a");
    undated_a.sort_order = 1;
    let mut done_old = task("done-old");
    done_old.is_completed = true;
    done_old.updated_at = Time::from_unix(10);
    let mut done_new = task("done-new");
    done_new.is_completed = true;
    done_new.updated_at = Time::from_unix(20);
    let ordered = order_tasks(
        &[
            done_old, undated_b, later, timed, done_new, undated_a, all_day,
        ],
        &z,
    );
    let ids: Vec<&str> = ordered.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        ids,
        ["allday", "timed", "later", "a", "b", "done-new", "done-old"]
    );
}

#[quench_test::test]
fn a_view_is_the_inbox_a_list_or_a_label() {
    let mut in_list = task("l");
    in_list.project_id = Some("p".into());
    let mut tagged = task("t");
    tagged.label_ids = vec!["x".into()];
    let tasks = [task("i"), in_list, tagged];
    assert_eq!(open_count(&tasks, &ListRef::Inbox), 2);
    assert_eq!(open_count(&tasks, &ListRef::Project("p".into())), 1);
    assert_eq!(open_count(&tasks, &ListRef::Label("x".into())), 1);
    assert_eq!(
        ListRef::parse(&ListRef::Label("x".into()).key()),
        ListRef::Label("x".into())
    );
    assert_eq!(ListRef::parse("junk"), ListRef::Inbox);
}

#[quench_test::test]
fn timed_presets_count_back_from_the_due_time() {
    let z = warsaw();
    let due = at(&z, 2026, 10, 28, 14, 30);
    assert_eq!(
        Preset::TenMinutes.at(due, false, &z),
        Some(at(&z, 2026, 10, 28, 14, 20))
    );
    assert_eq!(
        Preset::OneDay.at(due, false, &z),
        Some(at(&z, 2026, 10, 27, 14, 30))
    );
    assert_eq!(Preset::OnTheDay.at(due, false, &z), None);
}

#[quench_test::test]
fn all_day_presets_are_nine_local_before_the_utc_date() {
    let z = warsaw();
    let due = all_day_time(date(2026, 10, 28));
    assert_eq!(
        Preset::OnTheDay.at(due, true, &z),
        Some(at(&z, 2026, 10, 28, 9, 0))
    );
    assert_eq!(
        Preset::TwoDaysBefore.at(due, true, &z),
        Some(at(&z, 2026, 10, 26, 9, 0))
    );
    assert_eq!(
        Preset::detect(at(&z, 2026, 10, 27, 9, 0), due, true, &z),
        Some(Preset::DayBefore)
    );
}

#[quench_test::test]
fn the_hour_that_does_not_exist_moves_on() {
    let z = warsaw();
    // Clocks went forward on 2026-03-29 at 02:00.
    let t = at_local(date(2026, 3, 29), 2, 30, &z).expect("moved");
    assert_eq!(z.civil_at(t).hour, 3);
}

#[quench_test::test]
fn notification_ids_are_stable_and_positive() {
    assert_eq!(notification_id("abc"), notification_id("abc"));
    assert_ne!(notification_id("abc"), notification_id("abd"));
    assert!(notification_id("some-long-id-0123456789") >= 0);
    assert_eq!(notification_id(""), 0x811c_9dc5_u32 as i32 & 0x7FFF_FFFF);
}

#[quench_test::test]
fn only_open_future_reminders_are_scheduled_soonest_first() {
    let z = warsaw();
    let now = at(&z, 2026, 10, 28, 10, 0);
    let mut a = task("a");
    a.remind_at = Some(at(&z, 2026, 10, 30, 8, 0));
    a.due_at = Some(at(&z, 2026, 10, 30, 8, 30));
    let mut b = task("b");
    b.remind_at = Some(at(&z, 2026, 10, 29, 8, 0));
    let mut past = task("past");
    past.remind_at = Some(at(&z, 2026, 10, 27, 8, 0));
    let mut done = task("done");
    done.remind_at = Some(at(&z, 2026, 10, 31, 8, 0));
    done.is_completed = true;
    let got = pending_reminders(&[a, past, done, b], now, &z);
    let ids: Vec<&str> = got.iter().map(|r| r.task_id.as_str()).collect();
    assert_eq!(ids, ["b", "a"]);
    assert_eq!(got[1].body, "Due Fri 30 Oct 08:30");
    assert_eq!(got[0].body, "");
}

#[quench_test::test]
fn a_reminder_that_follows_the_due_date_moves_with_it() {
    let z = warsaw();
    let mut d = Draft::new(&ListRef::Inbox);
    d.content = "x".into();
    d.set_due(Due {
        date: date(2026, 10, 28),
        clock: Some((14, 30)),
    });
    assert!(d.set_preset(Preset::OneHour));
    assert_eq!(d.remind_at(&z), Some(at(&z, 2026, 10, 28, 13, 30)));
    d.set_due(Due {
        date: date(2026, 10, 30),
        clock: Some((8, 0)),
    });
    assert_eq!(d.remind_at(&z), Some(at(&z, 2026, 10, 30, 7, 0)));
}

#[quench_test::test]
fn a_preset_for_the_other_kind_of_date_is_dropped() {
    let z = warsaw();
    let mut d = Draft::new(&ListRef::Inbox);
    d.set_due(Due {
        date: date(2026, 10, 28),
        clock: Some((14, 30)),
    });
    d.set_preset(Preset::OneHour);
    d.set_due(Due {
        date: date(2026, 10, 28),
        clock: None,
    });
    assert_eq!(d.remind(), Remind::None);
    assert!(!d.set_preset(Preset::TenMinutes));
    assert!(d.set_preset(Preset::OnTheDay));
    assert_eq!(d.remind_at(&z), Some(at(&z, 2026, 10, 28, 9, 0)));
}

#[quench_test::test]
fn clearing_the_due_date_clears_the_repeat_and_a_preset_but_not_a_custom_reminder() {
    let mut d = Draft::new(&ListRef::Inbox);
    assert!(!d.set_recurrence(Some(Recurrence::every(Unit::Day, "UTC"))));
    d.set_due(Due {
        date: date(2026, 10, 28),
        clock: None,
    });
    assert!(d.set_recurrence(Some(Recurrence::every(Unit::Day, "UTC"))));
    d.set_preset(Preset::OnTheDay);
    d.clear_due();
    assert!(d.recurrence().is_none());
    assert_eq!(d.remind(), Remind::None);
    d.set_reminder_at(Time::from_unix(1_800_000_000));
    d.clear_due();
    assert_eq!(d.remind(), Remind::At(Time::from_unix(1_800_000_000)));
}

#[quench_test::test]
fn editing_a_task_finds_its_preset_again() {
    let z = warsaw();
    let mut t = task("x");
    t.due_at = Some(at(&z, 2026, 10, 28, 14, 30));
    t.remind_at = Some(at(&z, 2026, 10, 28, 14, 20));
    let d = Draft::from_task(&t, &z);
    assert_eq!(d.remind(), Remind::Preset(Preset::TenMinutes));
    assert_eq!(
        d.due(),
        Some(Due {
            date: date(2026, 10, 28),
            clock: Some((14, 30))
        })
    );
    t.remind_at = Some(at(&z, 2026, 10, 28, 14, 21));
    assert!(matches!(Draft::from_task(&t, &z).remind(), Remind::At(_)));
}

#[quench_test::test]
fn a_new_task_starts_in_the_view_being_looked_at() {
    let d = Draft::new(&ListRef::Project("p".into()));
    assert_eq!(d.project_id.as_deref(), Some("p"));
    let d = Draft::new(&ListRef::Label("l".into()));
    assert_eq!(d.label_ids, ["l"]);
    assert!(!d.can_save());
}

#[quench_test::test]
fn the_body_has_the_instants_and_trims_the_text() {
    let z = warsaw();
    let mut d = Draft::new(&ListRef::Inbox);
    d.content = "  Buy milk ".into();
    d.description = "  ".into();
    d.set_due(Due {
        date: date(2026, 10, 28),
        clock: None,
    });
    let body = d.body(&z, None, 0);
    assert_eq!(
        body.get("content").and_then(Value::as_str),
        Some("Buy milk")
    );
    assert!(body.get("description").unwrap().is_null());
    assert_eq!(
        body.get("due_at").and_then(Value::as_str),
        Some("2026-10-28T00:00:00Z")
    );
    assert_eq!(body.get("due_all_day").and_then(Value::as_bool), Some(true));
    assert!(body.get("is_completed").is_none());
}

#[quench_test::test]
fn the_calendar_starts_on_monday() {
    assert_eq!(month_title(2026, 10), "October 2026");
    assert_eq!(shift_month(2026, 12, 1), (2027, 1));
    assert_eq!(shift_month(2026, 1, -1), (2025, 12));
    let weeks = month_grid(2026, 10);
    // 1 October 2026 is a Thursday.
    assert_eq!(weeks[0][0], None);
    assert_eq!(weeks[0][3], Some(date(2026, 10, 1)));
    assert_eq!(weeks.len(), 5);
    assert_eq!(weeks[4][5], Some(date(2026, 10, 31)));
    assert_eq!(weeks[4][6], None);
}

#[quench_test::test]
fn the_widget_gets_every_list_and_the_open_tasks_in_display_order() {
    let z = warsaw();
    let mut later = task("b");
    later.due_at = Some(all_day_time(date(2026, 10, 30)));
    later.due_all_day = true;
    later.project_id = Some("p1".into());
    let mut soon = task("a");
    soon.due_at = Some(at(&z, 2026, 10, 28, 8, 0));
    soon.recurrence = Some(Recurrence::every(Unit::Day, "UTC"));
    soon.label_ids = vec!["l1".into()];
    let mut done = task("c");
    done.is_completed = true;
    let data = daybook_core::Data {
        projects: vec![daybook_core::Project {
            id: "p1".into(),
            name: "Home".into(),
            sort_order: 0,
        }],
        labels: vec![daybook_core::Label {
            id: "l1".into(),
            name: "chores".into(),
            color: "#22c55e".into(),
        }],
        tasks: vec![later, done, soon],
    };
    let json = daybook_core::widget::snapshot(&data, at(&z, 2026, 10, 28, 7, 0), &z);
    let v = json_value(&json);
    let lists: Vec<&str> = v
        .get("lists")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.get("id").and_then(Value::as_str).unwrap())
        .collect();
    assert_eq!(lists, ["inbox", "p:p1", "l:l1"]);
    let tasks = v.get("tasks").unwrap().as_array().unwrap();
    let ids: Vec<&str> = tasks
        .iter()
        .map(|t| t.get("id").and_then(Value::as_str).unwrap())
        .collect();
    assert_eq!(
        ids,
        ["a", "b"],
        "the completed one is not there, the soonest first"
    );
    assert_eq!(tasks[0].get("repeats").and_then(Value::as_bool), Some(true));
    assert_eq!(tasks[1].get("allDay").and_then(Value::as_bool), Some(true));
    assert_eq!(tasks[1].get("project").and_then(Value::as_str), Some("p1"));
    assert_eq!(
        tasks[0].get("due").and_then(Value::as_i64),
        Some(at(&z, 2026, 10, 28, 8, 0).timestamp_millis())
    );
}

fn json_value(text: &str) -> Value {
    quench_serde::json::from_str(text).unwrap()
}

#[quench_test::test]
fn a_widget_list_id_goes_both_ways() {
    use daybook_core::widget::{list_id, parse_list_id};
    for list in [
        ListRef::Inbox,
        ListRef::Project("p".into()),
        ListRef::Label("l".into()),
    ] {
        assert_eq!(parse_list_id(&list_id(&list)), Some(list));
    }
    assert_eq!(parse_list_id("p:"), None);
    assert_eq!(parse_list_id("junk"), None);
}

quench_test::main!();
