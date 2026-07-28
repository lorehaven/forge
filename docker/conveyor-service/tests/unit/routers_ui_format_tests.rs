//! Unit tests for `routers/ui/common/format.rs`.

use chrono::{Duration, Utc};
use conveyor_service::routers::ui::common::format;

#[test]
fn something_that_just_happened_reads_as_such() {
    assert_eq!(format::relative(Utc::now()), "just now");
    assert_eq!(
        format::relative(Utc::now() - Duration::seconds(30)),
        "just now"
    );
}

#[test]
fn minutes_hours_and_days_are_each_used_where_they_fit() {
    assert_eq!(
        format::relative(Utc::now() - Duration::minutes(3)),
        "3m ago"
    );
    assert_eq!(format::relative(Utc::now() - Duration::hours(5)), "5h ago");
    assert_eq!(format::relative(Utc::now() - Duration::days(9)), "9d ago");
}

#[test]
fn a_timestamp_in_the_future_does_not_read_as_negative() {
    // Clock skew between replicas can produce one, and "in -2s" would be worse
    // than rounding it to now.
    assert_eq!(
        format::relative(Utc::now() + Duration::seconds(30)),
        "just now"
    );
}

#[test]
fn durations_use_the_coarsest_unit_that_still_says_something() {
    assert_eq!(format::duration(0), "0s");
    assert_eq!(format::duration(42), "42s");
    assert_eq!(format::duration(90), "1m 30s");
    assert_eq!(format::duration(3600), "1h 0m");
    assert_eq!(format::duration(7860), "2h 11m");
}

#[test]
fn a_negative_duration_is_shown_as_unknown_rather_than_wrong() {
    assert_eq!(format::duration(-5), "-");
}

#[test]
fn a_finished_job_shows_how_long_it_took() {
    let started = Utc::now() - Duration::seconds(75);
    let finished = Utc::now() - Duration::seconds(15);
    assert_eq!(format::elapsed(Some(started), Some(finished)), "1m 0s");
}

#[test]
fn a_running_job_shows_how_long_it_has_been_going() {
    let started = Utc::now() - Duration::seconds(20);
    let rendered = format::elapsed(Some(started), None);
    assert!(rendered.ends_with("so far"), "{rendered}");
}

#[test]
fn a_job_that_never_started_shows_nothing_rather_than_zero() {
    // Zero would read as "it ran instantly", which is a different claim.
    assert_eq!(format::elapsed(None, None), "-");
    assert_eq!(format::elapsed(None, Some(Utc::now())), "-");
}
