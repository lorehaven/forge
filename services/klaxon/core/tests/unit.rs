//! When an alarm rings, which rings first, and the file the list is kept in.
#![allow(clippy::unwrap_used)]

use klaxon_core::{
    Alarm, Alarms, Now, WEEKDAYS, WEEKEND, clock, countdown, next, parse, render, repeat_words,
};

fn at(hour: u8, minute: u8, days: u8) -> Alarm {
    Alarm {
        hour,
        minute,
        days,
        ..Alarm::new(1)
    }
}

/// Wednesday (2) at 12:00.
const NOON: Now = Now {
    weekday: 2,
    minute: 720,
};

#[quench_test::test]
fn a_later_time_today_rings_today() {
    assert_eq!(at(13, 30, 0).days_ahead(NOON), Some(0));
    assert_eq!(at(13, 30, 0).minutes_until(NOON), Some(90));
}

#[quench_test::test]
fn a_time_that_has_passed_rings_tomorrow() {
    assert_eq!(at(7, 0, 0).days_ahead(NOON), Some(1));
    assert_eq!(at(7, 0, 0).minutes_until(NOON), Some(1440 - 300));
}

#[quench_test::test]
fn the_minute_that_is_now_has_passed() {
    assert_eq!(at(12, 0, 0).days_ahead(NOON), Some(1));
}

#[quench_test::test]
fn a_repeating_alarm_skips_to_its_day() {
    // Wednesday noon; the alarm is Saturday and Sunday only.
    assert_eq!(at(9, 0, WEEKEND).days_ahead(NOON), Some(3));
    // Wednesday only, and Wednesday's 09:00 is gone: next Wednesday.
    assert_eq!(at(9, 0, 0b100).days_ahead(NOON), Some(7));
    // Weekdays, later today.
    assert_eq!(at(18, 0, WEEKDAYS).days_ahead(NOON), Some(0));
}

#[quench_test::test]
fn an_alarm_that_is_off_never_rings() {
    let mut a = at(13, 0, 0);
    a.enabled = false;
    assert_eq!(a.days_ahead(NOON), None);
}

#[quench_test::test]
fn the_earliest_alarm_is_the_next_one() {
    let mut early = at(6, 30, WEEKDAYS);
    early.id = 2;
    let mut late = at(13, 0, 0);
    late.id = 3;
    let n = next(&[early, late], NOON).unwrap();
    assert_eq!(n.id, 3);
    assert_eq!((n.days_ahead, n.hour, n.minute), (0, 13, 0));
    // Once that one has rung and is off, tomorrow's 06:30 is next.
    let mut gone = at(13, 0, 0);
    gone.id = 3;
    gone.enabled = false;
    let n = next(&[at(6, 30, WEEKDAYS), gone], NOON).unwrap();
    assert_eq!((n.days_ahead, n.hour, n.minute), (1, 6, 30));
    assert_eq!(next(&[], NOON), None);
}

#[quench_test::test]
fn two_at_the_same_time_go_by_id() {
    let mut a = at(13, 0, 0);
    a.id = 5;
    let mut b = at(13, 0, 0);
    b.id = 4;
    assert_eq!(next(&[a, b], NOON).unwrap().id, 4);
}

#[quench_test::test]
fn a_one_time_alarm_switches_itself_off_when_it_rings() {
    let mut list = Alarms::default();
    list.put(at(8, 0, 0));
    let mut weekly = at(8, 0, WEEKDAYS);
    weekly.id = 2;
    list.put(weekly);
    list.fired(1);
    list.fired(2);
    assert!(!list.get(1).unwrap().enabled);
    assert!(list.get(2).unwrap().enabled);
}

#[quench_test::test]
fn the_list_survives_the_file() {
    let mut list = Alarms::default();
    let mut a = at(6, 45, WEEKDAYS);
    a.label = "Gym | then work".into();
    a.snooze = 5;
    a.vibrate = false;
    a.loud = true;
    a.sound = "content://media/internal/audio/media/42?title=a|b%".into();
    a.sound_name = "Morning | 100%".into();
    list.put(a);
    let mut b = at(23, 59, 0);
    b.id = list.fresh_id();
    b.enabled = false;
    list.put(b);
    assert_eq!(parse(&render(&list)), list);
}

#[quench_test::test]
fn a_line_that_does_not_read_is_left_out() {
    let text = "1|7|0|0|1|10|1|0|||ok\nnonsense\n2|25|0|0|1|10|1|0|||hour\n3|8|0|0|1|7|1|0|||odd snooze\n\
                4|9|0|0|1|10|1\n";
    let list = parse(text);
    assert_eq!(list.list.iter().map(|a| a.id).collect::<Vec<_>>(), [1, 3]);
    assert_eq!(list.get(3).unwrap().snooze, 10);
}

#[quench_test::test]
fn a_new_alarm_vibrates_quietly_with_the_default_sound() {
    let a = Alarm::new(1);
    assert!(a.vibrate && !a.loud && a.sound.is_empty());
}

#[quench_test::test]
fn what_rings_is_carried_to_the_next_alarm() {
    let mut a = at(13, 0, 0);
    a.vibrate = false;
    a.loud = true;
    a.sound = "content://x/1".into();
    let n = next(&[a], NOON).unwrap();
    assert!(!n.vibrate && n.loud);
    assert_eq!(n.sound, "content://x/1");
}

#[quench_test::test]
fn the_words() {
    assert_eq!(clock(7, 5), "07:05");
    assert_eq!(repeat_words(0), "Once");
    assert_eq!(repeat_words(0b111_1111), "Every day");
    assert_eq!(repeat_words(WEEKDAYS), "Weekdays");
    assert_eq!(repeat_words(WEEKEND), "Weekends");
    assert_eq!(repeat_words(0b001_0101), "Mon, Wed, Fri");
    assert_eq!(countdown(0), "Rings in less than a minute");
    assert_eq!(countdown(452), "Rings in 7 h 32 min");
    assert_eq!(countdown(1560), "Rings in 1 day 2 h");
}

#[quench_test::test]
fn new_ids_are_new() {
    let mut list = Alarms::default();
    assert_eq!(list.fresh_id(), 1);
    list.put(at(1, 0, 0));
    assert_eq!(list.fresh_id(), 2);
    list.remove(1);
    assert_eq!(list.fresh_id(), 1);
}

quench_test::main!();
