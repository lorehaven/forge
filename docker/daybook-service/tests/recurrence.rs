//! The repeat rule's arithmetic: pure functions, no database, an explicit clock.

use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::{Europe::Warsaw, Tz};
use daybook_service::domain::recurrence::{Mode, Recurrence, Unit};

fn rule(unit: Unit, interval: u32) -> Recurrence {
    Recurrence {
        unit,
        interval,
        weekdays: Vec::new(),
        mode: Mode::Fixed,
        timezone: "Europe/Warsaw".to_string(),
    }
}

/// A Warsaw wall-clock time as a UTC instant.
fn warsaw(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
    Warsaw
        .with_ymd_and_hms(y, m, d, h, min, 0)
        .unwrap()
        .with_timezone(&Utc)
}

/// What a UTC instant reads as in Warsaw, as `(y, m, d, h, min)`.
fn local(at: DateTime<Utc>) -> (i32, u32, u32, u32, u32) {
    use chrono::{Datelike, Timelike};
    let at = at.with_timezone(&Warsaw);
    (at.year(), at.month(), at.day(), at.hour(), at.minute())
}

/// Completing on the due date, with the schedule anchored there.
fn next(rule: &Recurrence, due: DateTime<Utc>, now: DateTime<Utc>) -> (i32, u32, u32, u32, u32) {
    local(rule.next_due(due, due, now).expect("a next occurrence"))
}

#[test]
fn a_daily_task_moves_to_the_next_day_at_the_same_time() {
    let due = warsaw(2026, 10, 22, 9, 0);
    assert_eq!(next(&rule(Unit::Day, 1), due, due), (2026, 10, 23, 9, 0));
    assert_eq!(next(&rule(Unit::Day, 3), due, due), (2026, 10, 25, 9, 0));
}

#[test]
fn finishing_early_moves_past_the_occurrence_just_done() {
    // Due the 30th, done on the 22nd: the next one is the 31st, not tomorrow.
    let due = warsaw(2026, 10, 30, 9, 0);
    let now = warsaw(2026, 10, 22, 12, 0);
    assert_eq!(next(&rule(Unit::Day, 1), due, now), (2026, 10, 31, 9, 0));
}

#[test]
fn finishing_late_skips_occurrences_already_past() {
    // A daily 09:00 task due on the 10th, done on the 22nd at 14:00: tomorrow, not the 11th.
    let due = warsaw(2026, 10, 10, 9, 0);
    let now = warsaw(2026, 10, 22, 14, 0);
    assert_eq!(next(&rule(Unit::Day, 1), due, now), (2026, 10, 23, 9, 0));

    // Before that day's own time it is still today.
    let morning = warsaw(2026, 10, 22, 7, 0);
    assert_eq!(
        next(&rule(Unit::Day, 1), due, morning),
        (2026, 10, 22, 9, 0)
    );
}

#[test]
fn a_fixed_weekly_task_stays_on_its_weekday_however_late_it_is_done() {
    // Due Monday the 19th, done on Wednesday the 21st: next Monday, the 26th.
    let due = warsaw(2026, 10, 19, 9, 0);
    let now = warsaw(2026, 10, 21, 10, 0);
    assert_eq!(next(&rule(Unit::Week, 1), due, now), (2026, 10, 26, 9, 0));
    assert_eq!(next(&rule(Unit::Week, 2), due, now), (2026, 11, 2, 9, 0));
}

#[test]
fn weekdays_pick_the_listed_days_of_every_nth_week() {
    let monday = warsaw(2026, 10, 19, 9, 0);
    let mut rule = rule(Unit::Week, 1);
    rule.weekdays = vec![1, 4]; // Monday and Thursday

    let thursday = rule.next_due(monday, monday, monday).unwrap();
    assert_eq!(local(thursday), (2026, 10, 22, 9, 0));
    let following = rule.next_due(monday, thursday, thursday).unwrap();
    assert_eq!(local(following), (2026, 10, 26, 9, 0));

    // Every second week: the weeks in between are skipped.
    rule.interval = 2;
    let thursday = rule.next_due(monday, monday, monday).unwrap();
    assert_eq!(local(thursday), (2026, 10, 22, 9, 0));
    let following = rule.next_due(monday, thursday, thursday).unwrap();
    assert_eq!(local(following), (2026, 11, 2, 9, 0));
}

#[test]
fn weekdays_before_the_anchor_in_its_own_week_are_not_used() {
    // Anchored on Wednesday with Mon/Fri listed: this week only Friday is left.
    let wednesday = warsaw(2026, 10, 21, 9, 0);
    let mut rule = rule(Unit::Week, 1);
    rule.weekdays = vec![1, 5];
    let first = rule.next_due(wednesday, wednesday, wednesday).unwrap();
    assert_eq!(local(first), (2026, 10, 23, 9, 0));
}

#[test]
fn a_monthly_task_on_the_31st_returns_to_the_31st_after_a_short_month() {
    let rule = rule(Unit::Month, 1);
    let anchor = warsaw(2026, 1, 31, 9, 0);

    let february = rule.next_due(anchor, anchor, anchor).unwrap();
    assert_eq!(
        local(february),
        (2026, 2, 28, 9, 0),
        "clamped to the month's end"
    );

    // Counted from the anchor, not from February's 28th, so it is not stuck there.
    let march = rule.next_due(anchor, february, february).unwrap();
    assert_eq!(local(march), (2026, 3, 31, 9, 0));
}

#[test]
fn a_yearly_task_on_a_leap_day_clamps_in_other_years() {
    let rule = rule(Unit::Year, 1);
    let anchor = warsaw(2028, 2, 29, 9, 0);
    let next = rule.next_due(anchor, anchor, anchor).unwrap();
    assert_eq!(local(next), (2029, 2, 28, 9, 0));
    let later = rule
        .next_due(anchor, warsaw(2031, 2, 28, 9, 0), warsaw(2031, 2, 28, 9, 0))
        .unwrap();
    assert_eq!(local(later), (2032, 2, 29, 9, 0));
}

#[test]
fn the_time_of_day_survives_the_clocks_going_back() {
    // Poland leaves summer time on 2026-10-25. 09:00 stays 09:00 on the wall clock, an hour
    // later in UTC - adding 24 hours would have made it 08:00.
    let due = warsaw(2026, 10, 24, 9, 0);
    let next = rule(Unit::Day, 1).next_due(due, due, due).unwrap();
    assert_eq!(local(next), (2026, 10, 25, 9, 0));
    assert_eq!(next, Utc.with_ymd_and_hms(2026, 10, 25, 8, 0, 0).unwrap());
}

#[test]
fn a_time_that_does_not_exist_lands_an_hour_later() {
    // Poland skips 02:00-03:00 on 2027-03-28; a 02:30 task goes to 03:30 that day, then back.
    let due = warsaw(2027, 3, 27, 2, 30);
    let rule = rule(Unit::Day, 1);
    let skipped = rule.next_due(due, due, due).unwrap();
    assert_eq!(local(skipped), (2027, 3, 28, 3, 30));
    let after = rule.next_due(due, skipped, skipped).unwrap();
    assert_eq!(local(after), (2027, 3, 29, 2, 30));
}

#[test]
fn after_completion_counts_from_the_day_it_is_done_at_the_original_time() {
    let mut rule = rule(Unit::Day, 3);
    rule.mode = Mode::AfterCompletion;
    // Due a week ago at 09:00, finished Wednesday afternoon: three days on, at 09:00.
    let due = warsaw(2026, 10, 14, 9, 0);
    let now = warsaw(2026, 10, 21, 15, 30);
    assert_eq!(next(&rule, due, now), (2026, 10, 24, 9, 0));

    // Finished early, the interval still counts from now.
    let early = warsaw(2026, 10, 12, 8, 0);
    assert_eq!(next(&rule, due, early), (2026, 10, 15, 9, 0));
}

#[test]
fn a_rule_is_evaluated_in_its_own_zone() {
    let mut rule = rule(Unit::Day, 1);
    rule.timezone = "America/New_York".to_string();
    let due = Utc.with_ymd_and_hms(2026, 10, 22, 13, 0, 0).unwrap(); // 09:00 in New York (EDT)
    let next = rule.next_due(due, due, due).unwrap();
    assert_eq!(next, Utc.with_ymd_and_hms(2026, 10, 23, 13, 0, 0).unwrap());
}

#[test]
fn validation_normalises_and_refuses() {
    let ok = Recurrence {
        weekdays: vec![4, 1, 4],
        ..rule(Unit::Week, 1)
    }
    .validated()
    .unwrap();
    assert_eq!(ok.weekdays, vec![1, 4], "sorted and de-duplicated");

    let bad = |r: Recurrence| r.validated().unwrap_err();
    assert!(bad(rule(Unit::Day, 0)).contains("interval"));
    assert!(bad(rule(Unit::Day, 1000)).contains("interval"));
    assert!(
        bad(Recurrence {
            timezone: "Mars/Olympus".to_string(),
            ..rule(Unit::Day, 1)
        })
        .contains("timezone")
    );
    assert!(
        bad(Recurrence {
            weekdays: vec![1],
            ..rule(Unit::Day, 1)
        })
        .contains("weekly")
    );
    assert!(
        bad(Recurrence {
            weekdays: vec![8],
            ..rule(Unit::Week, 1)
        })
        .contains("1 (Monday)")
    );
    assert!(
        bad(Recurrence {
            weekdays: vec![1],
            mode: Mode::AfterCompletion,
            ..rule(Unit::Week, 1)
        })
        .contains("fixed")
    );
}

#[test]
fn the_json_form_defaults_to_a_fixed_rule_in_utc() {
    let rule: Recurrence = serde_json::from_str(r#"{"unit":"week","interval":2}"#).unwrap();
    assert_eq!(rule.mode, Mode::Fixed);
    assert_eq!(rule.timezone, "UTC");
    assert!(rule.weekdays.is_empty());

    let json = serde_json::to_value(&rule).unwrap();
    assert_eq!(json["unit"], "week");
    assert_eq!(json["mode"], "fixed");
    assert!(serde_json::from_str::<Recurrence>(r#"{"unit":"fortnight","interval":1}"#).is_err());
    let _: Tz = "UTC".parse().unwrap();
}
