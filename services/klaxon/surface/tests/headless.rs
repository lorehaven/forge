//! The background commands against a directory of their own.
#![allow(clippy::unwrap_used)]

use klaxon_core::{Alarm, Alarms, Now, WEEKDAYS};
use klaxon_surface::{Store, command, next_json, read_now};

fn store(name: &str) -> Store {
    let dir = std::env::temp_dir().join(format!("klaxon-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    Store::at(dir)
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

const WEDNESDAY_NOON: Now = Now {
    weekday: 2,
    minute: 720,
};

#[quench_test::test]
fn the_time_is_read_from_date() {
    assert_eq!(
        read_now("3:07:05"),
        Some(Now {
            weekday: 2,
            minute: 425
        })
    );
    assert_eq!(read_now("7:23:59").unwrap().weekday, 6);
    assert_eq!(read_now("0:07:05"), None);
    assert_eq!(read_now("nonsense"), None);
}

#[quench_test::test]
fn nothing_saved_means_nothing_rings() {
    let s = store("empty");
    assert_eq!(
        command(&args(&["--next"]), &s, WEDNESDAY_NOON).unwrap(),
        "{\"none\":true}"
    );
}

#[quench_test::test]
fn next_names_the_alarm_that_rings_first() {
    let s = store("next");
    let mut list = Alarms::default();
    list.put(Alarm {
        hour: 6,
        label: "Say \"hi\"".into(),
        days: WEEKDAYS,
        ..Alarm::new(1)
    });
    s.save(&list);
    assert_eq!(
        command(&args(&["--next"]), &s, WEDNESDAY_NOON).unwrap(),
        "{\"id\":1,\"days_ahead\":1,\"hour\":6,\"minute\":0,\"snooze\":10,\"vibrate\":true,\"loud\":false,\"sound\":\"\",\"label\":\"Say \\\"hi\\\"\"}"
    );
}

#[quench_test::test]
fn what_rings_goes_to_kotlin_too() {
    let s = store("sound");
    let mut list = Alarms::default();
    list.put(Alarm {
        hour: 13,
        vibrate: false,
        loud: true,
        sound: "content://media/42".into(),
        ..Alarm::new(1)
    });
    s.save(&list);
    let json = command(&args(&["--next"]), &s, WEDNESDAY_NOON).unwrap();
    assert!(
        json.contains("\"vibrate\":false,\"loud\":true,\"sound\":\"content://media/42\""),
        "{json}"
    );
}

#[quench_test::test]
fn a_one_time_alarm_that_rang_is_off_and_the_next_one_is_named() {
    let s = store("fired");
    let mut list = Alarms::default();
    list.put(Alarm {
        hour: 12,
        minute: 1,
        ..Alarm::new(1)
    });
    list.put(Alarm {
        hour: 18,
        ..Alarm::new(2)
    });
    s.save(&list);
    let json = command(&args(&["--fired", "1"]), &s, WEDNESDAY_NOON).unwrap();
    assert!(json.starts_with("{\"id\":2,"), "{json}");
    assert!(!s.load().get(1).unwrap().enabled);
    // The change is on disk.
    assert!(s.load().get(2).unwrap().enabled);
}

#[quench_test::test]
fn anything_else_is_not_a_command() {
    let s = store("other");
    assert_eq!(command(&args(&[]), &s, WEDNESDAY_NOON), None);
    assert_eq!(command(&args(&["--fired", "x"]), &s, WEDNESDAY_NOON), None);
    assert_eq!(command(&args(&["--window"]), &s, WEDNESDAY_NOON), None);
    assert_eq!(next_json(None), "{\"none\":true}");
}

quench_test::main!();
