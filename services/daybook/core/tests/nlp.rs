//! Due dates written in a title.

use daybook_core::calendar::at_local;
use daybook_core::nlp::parse_due;
use quench_core::date::Date;
use quench_core::time::Time;
use quench_tz::Zone;

fn zone() -> Zone {
    Zone::named("Europe/Warsaw").unwrap()
}

/// Saturday 31 October 2026, 10:00 in Warsaw.
fn now() -> Time {
    at_local(Date::from_ymd(2026, 10, 31).unwrap(), 10, 0, &zone()).unwrap()
}

type Read = (String, String, Option<(u32, u32)>);

fn read(title: &str) -> Option<Read> {
    parse_due(title, now(), &zone()).map(|p| {
        let (y, m, d) = p.due.date.ymd();
        (p.content, format!("{y}-{m:02}-{d:02}"), p.due.clock)
    })
}

fn is(title: &str, content: &str, date: &str, clock: Option<(u32, u32)>) {
    assert_eq!(
        read(title),
        Some((content.to_string(), date.to_string(), clock)),
        "{title}"
    );
}

#[quench_test::test]
fn a_month_a_day_and_a_time() {
    is(
        "Pay rent nov 6 5pm",
        "Pay rent",
        "2026-11-06",
        Some((17, 0)),
    );
    is(
        "nov 6 5pm pay rent",
        "pay rent",
        "2026-11-06",
        Some((17, 0)),
    );
    is(
        "Pay rent on November 6th at 5:30 pm",
        "Pay rent",
        "2026-11-06",
        Some((17, 30)),
    );
    is("Pay rent 6 nov", "Pay rent", "2026-11-06", None);
    is(
        "Pay rent 6th of November 2027",
        "Pay rent",
        "2027-11-06",
        None,
    );
    is("Pay rent 2026-12-01", "Pay rent", "2026-12-01", None);
}

#[quench_test::test]
fn a_date_that_has_passed_is_next_year() {
    is("Tax oct 1", "Tax", "2027-10-01", None);
    is("Tax oct 31", "Tax", "2026-10-31", None);
}

#[quench_test::test]
fn relative_days() {
    is("Call mum today", "Call mum", "2026-10-31", None);
    is(
        "Call mum tomorrow at 9am",
        "Call mum",
        "2026-11-01",
        Some((9, 0)),
    );
    is("Call mum tonight", "Call mum", "2026-10-31", Some((20, 0)));
    is(
        "Call mum tonight at 22:15",
        "Call mum",
        "2026-10-31",
        Some((22, 15)),
    );
    is("Gym mon", "Gym", "2026-11-02", None);
    is("Gym on Saturday", "Gym", "2026-11-07", None);
    is("Gym next friday", "Gym", "2026-11-06", None);
    is("Gym next week", "Gym", "2026-11-02", None);
    is("Gym next month", "Gym", "2026-11-30", None);
    is("Gym this weekend", "Gym", "2026-11-07", None);
}

#[quench_test::test]
fn offsets() {
    is("Review in 3 days", "Review", "2026-11-03", None);
    is("Review in 2 weeks", "Review", "2026-11-14", None);
    is("Review in a month", "Review", "2026-11-30", None);
    is(
        "Review in 90 minutes",
        "Review",
        "2026-10-31",
        Some((11, 30)),
    );
    is("Review in 2 hours", "Review", "2026-10-31", Some((12, 0)));
}

#[quench_test::test]
fn a_time_alone_is_today_or_tomorrow() {
    is("Call at 5pm", "Call", "2026-10-31", Some((17, 0)));
    is("Call at 9am", "Call", "2026-11-01", Some((9, 0)));
    is("Call @ 17:45", "Call", "2026-10-31", Some((17, 45)));
    is("Call at 5", "Call", "2026-10-31", Some((17, 0)));
    is("Lunch noon", "Lunch", "2026-10-31", Some((12, 0)));
    is("Bed midnight", "Bed", "2026-11-01", Some((0, 0)));
    is("Call at 12am", "Call", "2026-11-01", Some((0, 0)));
}

#[quench_test::test]
fn words_that_are_not_dates_are_left_alone() {
    assert_eq!(read("Buy 5 apples"), None);
    assert_eq!(read("Call at the office"), None);
    assert_eq!(read("Room 101"), None);
    assert_eq!(read("Fix version 2026"), None);
    assert_eq!(read("Plan 13pm"), None);
    assert_eq!(read("Feb 30 party"), None);
    // The whole title is a date: it stays a title.
    assert_eq!(read("tomorrow"), None);
    assert_eq!(read(""), None);
}

#[quench_test::test]
fn the_phrase_is_reported() {
    let p = parse_due("Pay rent on nov 6 at 5pm!", now(), &zone()).unwrap();
    assert_eq!(p.content, "Pay rent");
    assert_eq!(p.phrase, "on nov 6 at 5pm");
}

#[quench_test::test]
fn unicode_titles_do_not_break_it() {
    is(
        "Zadzwonić do mamy jutro tomorrow",
        "Zadzwonić do mamy jutro",
        "2026-11-01",
        None,
    );
    is("Kup żółć 🍎 nov 6", "Kup żółć 🍎", "2026-11-06", None);
}

quench_test::main!();
