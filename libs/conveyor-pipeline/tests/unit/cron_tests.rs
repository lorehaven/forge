//! Unit tests for `cron.rs` and `on.schedule`: expressions, time zones, daylight-saving changes.

use conveyor_pipeline::{Cron, parse};
use quench_core::time::Time;
use quench_tz::Zone;

fn at(text: &str) -> Time {
    Time::parse_rfc3339(text).unwrap()
}

fn warsaw() -> Zone {
    Zone::named("Europe/Warsaw").unwrap()
}

fn next(expression: &str, after: &str, zone: &Zone) -> Option<String> {
    Cron::parse(expression)
        .unwrap()
        .next_after(at(after), zone)
        .map(|t| t.to_rfc3339())
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[quench_test::test]
fn steps_lists_and_ranges_pick_the_next_matching_minute() {
    let utc = Zone::utc();
    assert_eq!(next("*/15 * * * *", "2026-10-25T12:07:00Z", &utc).as_deref(), Some("2026-10-25T12:15:00Z"));
    assert_eq!(next("0,30 9-17 * * *", "2026-10-25T17:30:00Z", &utc).as_deref(), Some("2026-10-26T09:00:00Z"));
    assert_eq!(next("5-20/5 * * * *", "2026-10-25T12:06:00Z", &utc).as_deref(), Some("2026-10-25T12:10:00Z"));
    assert_eq!(next("10/20 * * * *", "2026-10-25T12:11:00Z", &utc).as_deref(), Some("2026-10-25T12:30:00Z"));
}

#[quench_test::test]
fn the_next_firing_is_strictly_after_the_reference_instant() {
    let utc = Zone::utc();
    assert_eq!(next("*/15 * * * *", "2026-10-25T12:15:00Z", &utc).as_deref(), Some("2026-10-25T12:30:00Z"));
    // Seconds into the matching minute do not make it fire again.
    assert_eq!(next("*/15 * * * *", "2026-10-25T12:15:30Z", &utc).as_deref(), Some("2026-10-25T12:30:00Z"));
}

#[quench_test::test]
fn macros_names_and_sunday_as_seven_work() {
    let utc = Zone::utc();
    assert_eq!(next("@daily", "2026-10-25T12:00:00Z", &utc).as_deref(), Some("2026-10-26T00:00:00Z"));
    assert_eq!(next("@hourly", "2026-10-25T12:00:00Z", &utc).as_deref(), Some("2026-10-25T13:00:00Z"));
    assert_eq!(next("@monthly", "2026-10-25T12:00:00Z", &utc).as_deref(), Some("2026-11-01T00:00:00Z"));
    // 2026-10-25 is a Sunday: `0` and `7` both mean it, `mon` is the next day.
    assert_eq!(next("0 12 * * 7", "2026-10-25T11:00:00Z", &utc).as_deref(), Some("2026-10-25T12:00:00Z"));
    assert_eq!(next("0 12 * * sun", "2026-10-25T11:00:00Z", &utc).as_deref(), Some("2026-10-25T12:00:00Z"));
    assert_eq!(next("0 12 * * MON", "2026-10-25T13:00:00Z", &utc).as_deref(), Some("2026-10-26T12:00:00Z"));
    assert_eq!(next("0 0 1 jan *", "2026-10-25T13:00:00Z", &utc).as_deref(), Some("2027-01-01T00:00:00Z"));
}

#[quench_test::test]
fn day_of_month_and_weekday_combine_with_or_when_both_are_restricted() {
    let utc = Zone::utc();
    // The 1st OR any Monday - not "a Monday that is the 1st".
    assert_eq!(next("0 0 1 * 1", "2026-10-25T13:00:00Z", &utc).as_deref(), Some("2026-10-26T00:00:00Z"));
    assert_eq!(next("0 0 1 * 1", "2026-10-26T13:00:00Z", &utc).as_deref(), Some("2026-11-01T00:00:00Z"));
}

#[quench_test::test]
fn leap_days_are_found_and_impossible_dates_are_not() {
    let utc = Zone::utc();
    assert_eq!(next("0 0 29 2 *", "2026-03-01T00:00:00Z", &utc).as_deref(), Some("2028-02-29T00:00:00Z"));
    assert_eq!(next("0 0 31 2 *", "2026-03-01T00:00:00Z", &utc), None);
}

#[quench_test::test]
fn malformed_expressions_say_which_field_is_wrong() {
    for (expression, needle) in [
        ("", "expected 5 fields"),
        ("* * * *", "expected 5 fields"),
        ("* * * * * *", "expected 5 fields"),
        ("60 * * * *", "minute"),
        ("* 24 * * *", "hour"),
        ("* * 0 * *", "day-of-month"),
        ("* * * 13 *", "month"),
        ("* * * * 8", "day-of-week"),
        ("*/0 * * * *", "step"),
        ("5-1 * * * *", "backwards"),
        ("a * * * *", "not a number"),
        ("@sometimes", "unknown schedule macro"),
    ] {
        let error = Cron::parse(expression).unwrap_err().to_string();
        assert!(error.contains(needle), "`{expression}` -> {error}");
    }
}

// ---------------------------------------------------------------------------
// Time zones
// ---------------------------------------------------------------------------

#[quench_test::test]
fn local_midnight_tracks_the_zone_through_summer_and_winter_time() {
    let zone = warsaw();
    // CEST (UTC+2): midnight local is 22:00Z the day before.
    assert_eq!(next("0 0 * * *", "2026-07-01T10:00:00Z", &zone).as_deref(), Some("2026-07-01T22:00:00Z"));
    // CET (UTC+1): 23:00Z.
    assert_eq!(next("0 0 * * *", "2026-12-01T10:00:00Z", &zone).as_deref(), Some("2026-12-01T23:00:00Z"));
}

#[quench_test::test]
fn the_firing_after_the_clocks_go_back_uses_the_new_offset() {
    // 2026-10-25: CEST ends at 01:00Z. Midnight of the 26th is already CET -> 23:00Z on the 25th.
    assert_eq!(next("0 0 * * *", "2026-10-25T10:00:00Z", &warsaw()).as_deref(), Some("2026-10-25T23:00:00Z"));
    // And the one before it, still CEST -> 22:00Z.
    assert_eq!(next("0 0 * * *", "2026-10-24T12:00:00Z", &warsaw()).as_deref(), Some("2026-10-24T22:00:00Z"));
}

#[quench_test::test]
fn a_local_time_that_does_not_exist_is_skipped_that_day() {
    // 2026-03-29: at 02:00 CET clocks jump to 03:00 CEST, so 02:30 never happens.
    let zone = warsaw();
    assert_eq!(next("30 2 * * *", "2026-03-28T12:00:00Z", &zone).as_deref(), Some("2026-03-30T00:30:00Z"));
}

#[quench_test::test]
fn a_local_time_that_happens_twice_fires_at_both_instants() {
    // 2026-10-25: 02:30 occurs in CEST (00:30Z) and again in CET (01:30Z).
    let zone = warsaw();
    let first = next("30 2 * * *", "2026-10-24T12:00:00Z", &zone).unwrap();
    assert_eq!(first, "2026-10-25T00:30:00Z");
    let second = next("30 2 * * *", &first, &zone).unwrap();
    assert_eq!(second, "2026-10-25T01:30:00Z");
}

#[quench_test::test]
fn every_firing_over_two_years_is_in_order_and_on_the_local_clock() {
    // Walk 2 years of nightly firings: monotonic, ~one a day, and always 00:00 on the local clock.
    let zone = warsaw();
    let cron = Cron::parse("0 0 * * *").unwrap();
    let mut at = at("2026-01-01T00:00:00Z");
    for _ in 0..730 {
        let fired = cron.next_after(at, &zone).unwrap();
        assert!(fired > at);
        let local = zone.civil_at(fired);
        assert_eq!((local.hour, local.minute), (0, 0), "{}", fired.to_rfc3339());
        let gap = fired.timestamp() - at.timestamp();
        assert!(gap <= 25 * 3600 + 60, "gap of {gap}s before {}", fired.to_rfc3339());
        at = fired;
    }
}

// ---------------------------------------------------------------------------
// `on.schedule` in a pipeline file
// ---------------------------------------------------------------------------

const STAGE: &str = "\n[[stage]]\nname = \"s\"\n[[stage.job]]\nname = \"j\"\nsteps = [{ run = \"true\" }]\n";

#[quench_test::test]
fn a_pipeline_declares_schedules_and_a_zone() {
    let spec = parse(&format!(
        "on = {{ push = [\"master\"], schedule = [\"0 0 * * *\", \"@weekly\"], timezone = \"Europe/Warsaw\" }}{STAGE}"
    ))
    .unwrap();
    assert_eq!(spec.schedules.len(), 2);
    assert_eq!(spec.schedules[0].expression, "0 0 * * *");
    assert_eq!(spec.schedules[0].timezone, "Europe/Warsaw");
    assert_eq!(
        spec.schedules[0].next_after(at("2026-07-01T10:00:00Z")).unwrap().to_rfc3339(),
        "2026-07-01T22:00:00Z"
    );
}

#[quench_test::test]
fn the_zone_defaults_to_utc_and_no_schedule_means_none() {
    let spec = parse(&format!("on = {{ schedule = [\"0 3 * * *\"] }}{STAGE}")).unwrap();
    assert_eq!(spec.schedules[0].timezone, "UTC");
    assert!(parse(&format!("on = {{ push = [\"*\"] }}{STAGE}")).unwrap().schedules.is_empty());
    assert!(parse(STAGE).unwrap().schedules.is_empty());
}

#[quench_test::test]
fn a_schedule_that_can_never_fire_is_a_parse_error() {
    for (on, needle) in [
        ("on = { schedule = [\"0 0 31 2 *\"] }", "never fires"),
        ("on = { schedule = [\"nope\"] }", "expected 5 fields"),
        ("on = { schedule = [\"0 0 * * *\"], timezone = \"Mars/Olympus\" }", "unknown time zone"),
    ] {
        let error = parse(&format!("{on}{STAGE}")).unwrap_err().to_string();
        assert!(error.contains(needle), "{on} -> {error}");
    }
}

#[quench_test::test]
fn the_schedule_event_runs_only_when_the_pipeline_declares_one() {
    let scheduled = parse(&format!("on = {{ schedule = [\"0 0 * * *\"] }}{STAGE}")).unwrap();
    assert!(scheduled.allows("schedule", "master"));
    // A schedule is not a push: declaring one does not make pushes build.
    assert!(!scheduled.allows("push", "master"));

    let plain = parse(&format!("on = {{ push = [\"*\"] }}{STAGE}")).unwrap();
    assert!(!plain.allows("schedule", "master"));
    assert!(plain.allows("push", "master"));
    assert!(plain.allows("manual", "master"));
}

#[quench_test::test]
fn when_conditions_can_tell_a_scheduled_run_from_the_rest() {
    use conveyor_pipeline::{Condition, EvalContext};
    let only_nightly = Condition::parse("event == 'schedule'").unwrap();
    let not_nightly = Condition::parse("event != 'schedule'").unwrap();
    let nightly = EvalContext::new("schedule", "master", "abc");
    let push = EvalContext::new("push", "master", "abc");
    assert!(only_nightly.evaluate(&nightly) && !not_nightly.evaluate(&nightly));
    assert!(!only_nightly.evaluate(&push) && not_nightly.evaluate(&push));
}

// ---------------------------------------------------------------------------
// The real repositories' pipelines: the nightly run must be the security stage and nothing else
// ---------------------------------------------------------------------------

fn stages_that_run(source: &str, event: &str) -> Vec<String> {
    use conveyor_pipeline::{EvalContext, plan};
    let spec = parse(source).expect("the repository's own pipeline must parse");
    plan(&spec, &EvalContext::new(event, "refs/heads/master", "abc"))
        .into_iter()
        .filter(|stage| stage.decision.will_run())
        .map(|stage| spec.stages[stage.index].name.clone())
        .collect()
}

#[quench_test::test]
fn forge_and_quench_run_only_the_security_stage_when_the_clock_starts_them() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let pipelines = [
        ("forge", root.join("../../.conveyor.toml")),
        // A sibling checkout; absent when forge is built on its own.
        ("quench", root.join("../../../quench/.conveyor.toml")),
    ];
    let mut checked = 0;
    for (name, path) in pipelines {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        checked += 1;

        assert_eq!(stages_that_run(&source, "schedule"), ["security"], "{name}: nightly");
        for event in ["push", "pull_request", "manual"] {
            let ran = stages_that_run(&source, event);
            assert!(!ran.contains(&"security".to_string()), "{name}: {event} must not run the nightly stage: {ran:?}");
            assert_eq!(ran, ["check", "build", "test"], "{name}: {event} is what it always was");
        }

        let spec = parse(&source).unwrap();
        assert_eq!(spec.schedules.len(), 1, "{name}");
        assert_eq!(spec.schedules[0].timezone, "Europe/Warsaw");
        // And it is a midnight, whatever the season.
        for when in ["2026-07-01T10:00:00Z", "2026-12-01T10:00:00Z"] {
            let fires = spec.schedules[0].next_after(at(when)).unwrap();
            let local = Zone::named("Europe/Warsaw").unwrap().civil_at(fires);
            assert_eq!((local.hour, local.minute), (0, 0), "{name} {when}");
        }
    }
    assert!(checked >= 1, "no repository pipeline was found to check");
}
