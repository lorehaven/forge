//! The screens with no host: mounted into a test tree, driven by events, read back as text.
#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::rc::Rc;

use klaxon_app::live::{Klaxon, Msg, Request, Setup, klaxon};
use klaxon_core::{Alarm, Now, WEEKDAYS};
use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::test_renderer::NodeId;

type Asked = Rc<RefCell<Vec<Request>>>;

/// What the host keeps of the app once it is mounted.
struct Hooks {
    deliver: Rc<dyn Fn(Msg)>,
    back: Rc<dyn Fn() -> bool>,
}

type Live = (
    Rc<TestRenderer>,
    Asked,
    Hooks,
    quench_ui::Mount<TestRenderer>,
);

/// Wednesday at 06:00.
const MORNING: Now = Now {
    weekday: 2,
    minute: 360,
};

fn find(r: &TestRenderer, id: &str) -> NodeId {
    r.document_order()
        .into_iter()
        .find(|n| r.attr(*n, "id").as_deref() == Some(id))
        .unwrap_or_else(|| panic!("no #{id}: {}", r.html()))
}

fn present(r: &TestRenderer, id: &str) -> bool {
    r.document_order()
        .into_iter()
        .any(|n| r.attr(n, "id").as_deref() == Some(id))
}

fn click(r: &TestRenderer, id: &str) {
    r.fire(find(r, id), &Event::Click);
}

fn alarm(id: u32, hour: u8, minute: u8, days: u8, label: &str) -> Alarm {
    Alarm {
        id,
        hour,
        minute,
        days,
        label: label.into(),
        ..Alarm::new(id)
    }
}

fn live(alarms: Vec<Alarm>, alerts_ok: bool) -> Live {
    // Every test gets a clock of its own: the platform is per thread and the runner reuses threads.
    quench_ui::platform::install(Rc::new(quench_ui::platform::ManualPlatform::new()));
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let Klaxon {
        view,
        deliver,
        back,
    } = klaxon(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            alarms,
            now: MORNING,
            alerts_ok,
            toast_ms: 0,
        },
    );
    let r = Rc::new(TestRenderer::new());
    let mounted = mount(&r, &r.root(), view);
    (r, asked, Hooks { deliver, back }, mounted)
}

#[quench_test::test]
fn with_no_alarm_the_list_says_so() {
    let (r, _, _, _m) = live(Vec::new(), true);
    assert!(r.text().contains("No alarms yet"), "{}", r.text());
    assert!(r.text().contains("No alarm is set"));
}

#[quench_test::test]
fn the_list_is_by_time_and_says_when_the_next_one_rings() {
    let (r, _, _, _m) = live(
        vec![alarm(1, 9, 30, WEEKDAYS, "Standup"), alarm(2, 6, 45, 0, "")],
        true,
    );
    let text = r.text();
    assert!(
        text.find("06:45").unwrap() < text.find("09:30").unwrap(),
        "{text}"
    );
    assert!(
        text.contains("Standup \u{b7} Weekdays") && text.contains("Once"),
        "{text}"
    );
    // 06:00 now, 06:45 next.
    assert!(text.contains("Rings in 45 min"), "{text}");
}

#[quench_test::test]
fn a_switch_turns_an_alarm_off_and_tells_the_host() {
    let (r, asked, _, _m) = live(vec![alarm(1, 6, 45, 0, "")], true);
    click(&r, "toggle-1");
    assert_eq!(*asked.borrow(), [Request::Toggle(1, false)]);
    assert!(r.text().contains("No alarm is set"), "{}", r.text());
    assert_eq!(
        r.attr(find(&r, "toggle-1"), "aria-checked").as_deref(),
        Some("false")
    );
}

#[quench_test::test]
fn a_new_alarm_is_made_with_the_steppers_and_saved() {
    let (r, asked, _, _m) = live(vec![alarm(4, 6, 45, 0, "")], true);
    click(&r, "add");
    assert!(r.text().contains("New alarm"), "{}", r.text());
    assert!(!present(&r, "delete"), "nothing to delete yet");
    // 07:00 to 08:10, Mon and Tue, labelled.
    click(&r, "hour-up1");
    click(&r, "minute-up5");
    click(&r, "minute-up5");
    click(&r, "day-0");
    click(&r, "day-1");
    r.fire(
        r.document_order()
            .into_iter()
            .find(|n| r.tag(*n).as_deref() == Some("input"))
            .unwrap(),
        &Event::Input(" Run ".into()),
    );
    assert_eq!(r.text_of(find(&r, "hour-value")), "08");
    assert_eq!(r.text_of(find(&r, "minute-value")), "10");
    assert!(r.text().contains("Mon, Tue"));
    click(&r, "save");
    assert_eq!(
        *asked.borrow(),
        [Request::Save(Alarm {
            id: 5,
            hour: 8,
            minute: 10,
            label: "Run".into(),
            days: 0b11,
            enabled: true,
            snooze: 10,
            vibrate: true,
            loud: false,
            sound: String::new(),
            sound_name: String::new(),
        })]
    );
    assert!(
        r.text().contains("08:10") && r.text().contains("Alarm set for 08:10"),
        "{}",
        r.text()
    );
}

#[quench_test::test]
fn vibration_and_loudness_are_chosen_per_alarm() {
    let (r, asked, _, _m) = live(Vec::new(), true);
    click(&r, "add");
    assert_eq!(
        r.attr(find(&r, "vibrate"), "aria-checked").as_deref(),
        Some("true")
    );
    assert_eq!(
        r.attr(find(&r, "loud"), "aria-checked").as_deref(),
        Some("false")
    );
    click(&r, "vibrate");
    click(&r, "loud");
    click(&r, "save");
    let asked = asked.borrow();
    let [Request::Save(a)] = asked.as_slice() else {
        panic!("{asked:?}")
    };
    assert!(!a.vibrate && a.loud);
}

#[quench_test::test]
fn the_sound_is_chosen_by_the_system_s_picker_and_kept() {
    let (r, asked, app, _m) = live(Vec::new(), true);
    click(&r, "add");
    assert!(r.text_of(find(&r, "sound")).contains("Default alarm sound"));
    click(&r, "sound");
    assert_eq!(*asked.borrow(), [Request::PickSound(String::new())]);
    (app.deliver)(Msg::Sound {
        uri: "content://media/42".into(),
        name: "Morning Dew".into(),
    });
    assert!(
        r.text_of(find(&r, "sound")).contains("Morning Dew"),
        "{}",
        r.text()
    );
    // The picker is asked again with the sound that is set, so it opens on it.
    click(&r, "sound");
    assert_eq!(
        asked.borrow()[1],
        Request::PickSound("content://media/42".into())
    );
    click(&r, "save");
    let asked = asked.borrow();
    let Request::Save(a) = &asked[2] else {
        panic!("{asked:?}")
    };
    assert_eq!(
        (a.sound.as_str(), a.sound_name.as_str()),
        ("content://media/42", "Morning Dew")
    );
}

#[quench_test::test]
fn a_sound_that_arrives_with_no_editor_open_is_dropped() {
    let (r, _, app, _m) = live(Vec::new(), true);
    (app.deliver)(Msg::Sound {
        uri: "x".into(),
        name: "y".into(),
    });
    assert!(r.text().contains("No alarms yet"));
}

#[quench_test::test]
fn the_steppers_wrap_around() {
    let (r, _, _, _m) = live(Vec::new(), true);
    click(&r, "add");
    for _ in 0..8 {
        click(&r, "hour-down1");
    }
    assert_eq!(r.text_of(find(&r, "hour-value")), "23");
    click(&r, "minute-down1");
    assert_eq!(r.text_of(find(&r, "minute-value")), "59");
}

#[quench_test::test]
fn an_alarm_opens_in_the_editor_and_can_be_deleted() {
    let (r, asked, _, _m) = live(vec![alarm(1, 6, 45, WEEKDAYS, "Gym")], true);
    r.fire(
        r.document_order()
            .into_iter()
            .find(|n| r.attr(*n, "class").as_deref() == Some("klaxon-row-main"))
            .unwrap(),
        &Event::Click,
    );
    assert!(
        r.text().contains("Edit alarm") && r.text().contains("06"),
        "{}",
        r.text()
    );
    click(&r, "delete");
    assert_eq!(*asked.borrow(), [Request::Delete(1)]);
    assert!(r.text().contains("No alarms yet"));
}

#[quench_test::test]
fn back_closes_the_editor_and_then_leaves() {
    let (r, asked, app, _m) = live(Vec::new(), true);
    assert!(!(app.back)(), "nothing open: the host leaves");
    click(&r, "add");
    assert!((app.back)());
    assert!(present(&r, "alarms"));
    click(&r, "add");
    click(&r, "cancel");
    assert!(present(&r, "alarms"));
    assert!(asked.borrow().is_empty());
}

#[quench_test::test]
fn the_banner_asks_for_what_the_lock_screen_needs_until_it_is_given() {
    let (r, asked, app, _m) = live(Vec::new(), false);
    assert!(present(&r, "alerts-banner"));
    click(&r, "allow-alerts");
    assert_eq!(*asked.borrow(), [Request::AllowAlerts]);
    (app.deliver)(Msg::Alerts(true));
    assert!(!present(&r, "alerts-banner"));
}

#[quench_test::test]
fn the_host_s_list_and_clock_replace_what_the_screen_shows() {
    let (r, _, app, _m) = live(vec![alarm(1, 6, 45, 0, "")], true);
    // The one-time alarm rang and switched itself off.
    let mut off = alarm(1, 6, 45, 0, "");
    off.enabled = false;
    (app.deliver)(Msg::Alarms(vec![off]));
    assert!(r.text().contains("No alarm is set"));
    (app.deliver)(Msg::Alarms(vec![alarm(1, 6, 45, 0, "")]));
    assert!(r.text().contains("Rings in 45 min"));
    (app.deliver)(Msg::Clock(Now {
        weekday: 2,
        minute: 390,
    }));
    assert!(r.text().contains("Rings in 15 min"), "{}", r.text());
}

quench_test::main!();
