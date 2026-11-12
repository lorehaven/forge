use crate::support::*;
use quench_core::time::Time;
use songbook_core::Schedule;
use songbook_service::schedule::{check, due, load, next_run, parse_time, save};

fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> Time {
    Time::from_ymd_hms(y, mo, d, h, mi, 0).unwrap()
}

fn nightly(zone: &str) -> Schedule {
    Schedule {
        enabled: true,
        time: "03:00".into(),
        zone: zone.into(),
        ..Schedule::default()
    }
}

#[quench_test::test]
fn a_time_is_hh_mm() {
    assert_eq!(parse_time("03:00"), Some((3, 0)));
    assert_eq!(parse_time("3:05"), Some((3, 5)));
    assert_eq!(parse_time("23:59"), Some((23, 59)));
    for bad in ["24:00", "12:60", "noon", "12", "", "1:2:3", "-1:00"] {
        assert_eq!(parse_time(bad), None, "{bad}");
    }
}

#[quench_test::test]
fn it_is_due_once_a_day_after_the_time_in_its_zone() {
    let s = nightly("UTC");
    assert!(!due(&s, at(2026, 11, 2, 2, 59)));
    assert!(due(&s, at(2026, 11, 2, 3, 0)));
    assert!(
        due(&s, at(2026, 11, 2, 9, 0)),
        "a busy night catches up later the same day"
    );
    let ran = Schedule {
        last_day: "2026-11-02".into(),
        ..s.clone()
    };
    assert!(!due(&ran, at(2026, 11, 2, 9, 0)));
    assert!(due(&ran, at(2026, 11, 3, 3, 0)));
    let off = Schedule {
        enabled: false,
        ..s
    };
    assert!(!due(&off, at(2026, 11, 2, 9, 0)));
}

#[quench_test::test]
fn the_zone_decides_what_the_time_means() {
    // 03:00 in Warsaw in November is 02:00 UTC; in July it is 01:00 UTC.
    let s = nightly("Europe/Warsaw");
    assert!(!due(&s, at(2026, 11, 2, 1, 59)));
    assert!(due(&s, at(2026, 11, 2, 2, 0)));
    assert!(!due(&s, at(2026, 7, 1, 0, 59)));
    assert!(due(&s, at(2026, 7, 1, 1, 0)));
}

#[quench_test::test]
fn the_next_run_is_today_until_it_has_started_then_tomorrow() {
    let s = nightly("UTC");
    assert_eq!(next_run(&s, at(2026, 11, 2, 1, 0)), "2026-11-02 03:00");
    let ran = Schedule {
        last_day: "2026-11-02".into(),
        ..s.clone()
    };
    assert_eq!(next_run(&ran, at(2026, 11, 2, 4, 0)), "2026-11-03 03:00");
    assert_eq!(
        next_run(
            &Schedule {
                enabled: false,
                ..s
            },
            at(2026, 11, 2, 1, 0)
        ),
        ""
    );
}

#[quench_test::test]
fn a_schedule_must_be_valid() {
    assert!(check(nightly("UTC")).is_ok());
    assert_eq!(
        check(Schedule {
            time: "7:5".into(),
            ..nightly("")
        })
        .unwrap()
        .time,
        "07:05"
    );
    assert!(
        check(Schedule {
            time: "25:00".into(),
            ..nightly("")
        })
        .is_err()
    );
    assert!(check(nightly("Mars/Olympus")).is_err());
    let none = Schedule {
        sync: false,
        links: false,
        download: false,
        convert: false,
        suggestions: false,
        images: false,
        ..nightly("")
    };
    assert!(check(none).is_err());
}

#[quench_test::test]
async fn a_saved_schedule_is_kept_and_a_passed_time_waits_for_tomorrow() {
    if !have_database() {
        return;
    }
    let dir = Scratch::new();
    let app = app_with(&dir, &[]).await;
    let first = load(&app).await.unwrap();
    assert!(!first.enabled);
    assert_eq!(first.next_run, "");

    // 00:00 has always passed today, so saving it must not start a run at once.
    let saved = save(
        &app,
        Schedule {
            time: "00:00".into(),
            zone: "UTC".into(),
            ..nightly("UTC")
        },
    )
    .await
    .unwrap();
    assert!(saved.enabled);
    assert!(!saved.last_day.is_empty(), "today is used up");
    assert!(!due(&saved, Time::now()));
    assert_eq!(load(&app).await.unwrap().time, "00:00");
    assert!(
        save(
            &app,
            Schedule {
                time: "x".into(),
                ..nightly("UTC")
            }
        )
        .await
        .is_err()
    );
}
