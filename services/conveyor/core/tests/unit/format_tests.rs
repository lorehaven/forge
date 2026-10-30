use crate::support::at;
use conveyor_core::format::{
    duration, elapsed, page_count, relative, segment, short_digest, stamp,
};
use quench_core::time::Duration;

#[quench_test::test]
fn something_that_just_happened_reads_as_such() {
    let now = at("2026-10-29T12:00:00Z");
    assert_eq!(relative(now, now), "just now");
    assert_eq!(relative(now - Duration::seconds(30), now), "just now");
}

#[quench_test::test]
fn minutes_hours_and_days_are_each_used_where_they_fit() {
    let now = at("2026-10-29T12:00:00Z");
    assert_eq!(relative(now - Duration::minutes(3), now), "3m ago");
    assert_eq!(relative(now - Duration::hours(5), now), "5h ago");
    assert_eq!(relative(now - Duration::days(9), now), "9d ago");
}

#[quench_test::test]
fn a_timestamp_in_the_future_does_not_read_as_negative() {
    let now = at("2026-10-29T12:00:00Z");
    assert_eq!(relative(now + Duration::seconds(30), now), "just now");
}

#[quench_test::test]
fn durations_use_the_coarsest_unit_that_still_says_something() {
    assert_eq!(duration(0), "0s");
    assert_eq!(duration(42), "42s");
    assert_eq!(duration(90), "1m 30s");
    assert_eq!(duration(3600), "1h 0m");
    assert_eq!(duration(7860), "2h 11m");
    assert_eq!(duration(-5), "-");
}

#[quench_test::test]
fn elapsed_is_the_gap_so_far_or_nothing() {
    let now = at("2026-10-29T12:00:00Z");
    let started = now - Duration::seconds(75);
    let finished = now - Duration::seconds(15);
    assert_eq!(elapsed(Some(started), Some(finished), now), "1m 0s");
    assert_eq!(
        elapsed(Some(now - Duration::seconds(20)), None, now),
        "20s so far"
    );
    assert_eq!(elapsed(None, None, now), "-");
    assert_eq!(elapsed(None, Some(now), now), "-");
}

#[quench_test::test]
fn digests_and_stamps_are_short() {
    assert_eq!(
        short_digest("sha256:0123456789abcdef0123"),
        "sha256:0123456789ab…"
    );
    assert_eq!(short_digest("plain"), "plain");
    assert_eq!(short_digest("sha256:ab"), "sha256:ab…");
    assert_eq!(stamp(at("2026-10-29T10:20:30Z")), "2026-10-29 10:20 UTC");
}

#[quench_test::test]
fn an_empty_history_is_still_page_one_of_one() {
    assert_eq!(page_count(0, 25), 1);
    assert_eq!(page_count(26, 25), 2);
    assert_eq!(page_count(25, 25), 1);
    assert_eq!(page_count(50, 25), 2);
    assert_eq!(page_count(5, 0), 5);
}

#[quench_test::test]
fn segments_are_percent_encoded() {
    assert_eq!(segment("a b/c"), "a%20b%2Fc");
    assert_eq!(segment("forge-1.x_y~"), "forge-1.x_y~");
}
