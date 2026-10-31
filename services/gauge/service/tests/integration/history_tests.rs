//! The stored history, against a real Postgres.
//!
//! Skipped unless `GAUGE_TEST_DATABASE_URL` is set; point it at a throwaway database, the tests empty
//! `gauge.samples`.
//!
//! ```bash
//! docker run --rm -d --name gauge-test-pg -e POSTGRES_PASSWORD=postgres -p 55433:5432 postgres:18-alpine
//! cargo run -p foundry -- apply --catalog services/foundry/service/migrations \
//!     --database-url postgres://postgres:postgres@localhost:55433/postgres --install gauge
//! GAUGE_TEST_DATABASE_URL=postgres://postgres:postgres@localhost:55433/postgres cargo test -p gauge
//! ```

use gauge_core::Range;
use gauge_service::domain::history::{self, Bucket, FIELDS, History};
use gauge_service::domain::model::Filesystem;
use gauge_service::domain::store::{ForecastCell, Note, Point, Tick};
use quench_db::prelude::{Database, Db};
use std::sync::OnceLock;
use tokio::sync::{Mutex, MutexGuard};

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

async fn database() -> Option<(History, MutexGuard<'static, ()>)> {
    let url = std::env::var("GAUGE_TEST_DATABASE_URL").ok()?;
    if url.trim().is_empty() {
        return None;
    }
    let guard = lock().lock().await;
    let db = Db::connect(&url).await.expect("connect");
    db.execute("TRUNCATE gauge.samples, gauge.filesystem_samples, gauge.alert_events")
        .await
        .expect("truncate");
    Some((History::connect(&url).await.expect("history"), guard))
}

fn skipped(test: &str) {
    println!("skipping {test}: GAUGE_TEST_DATABASE_URL is not set");
}

/// A minute whose mean cpu is `cpu` and whose highest is `peak`; everything else unreported.
const fn bucket(minute: i64, cpu: f64, peak: f64) -> Bucket {
    let mut mean = [None; FIELDS.len()];
    let mut max = [None; FIELDS.len()];
    mean[0] = Some(cpu);
    max[0] = Some(peak);
    Bucket {
        minute,
        scrapes: 6,
        mean,
        max,
    }
}

#[quench_test::test]
async fn a_stored_minute_comes_back_in_its_step_and_the_rest_are_gaps() {
    let Some((history, _guard)) = database().await else {
        return skipped("a_stored_minute_comes_back_in_its_step_and_the_rest_are_gaps");
    };
    let now: f64 = 1_800_000_000.0;
    let minute = (now / 60.0).floor() as i64 * 60 - 3600;
    history
        .write("lorien", &bucket(minute, 40.0, 90.0))
        .await
        .expect("write");
    history
        .write("lorien", &bucket(minute + 60, 60.0, 70.0))
        .await
        .expect("write");
    history
        .write("nogrod", &bucket(minute, 5.0, 5.0))
        .await
        .expect("write");

    let points = history
        .read("lorien", Range::Day, now, false)
        .await
        .expect("read");
    assert_eq!(points.len(), 288, "a day in five-minute steps");
    let filled: Vec<_> = points.iter().filter(|p| p.cpu.is_some()).collect();
    assert_eq!(filled.len(), 1, "both minutes fall in one step");
    assert!(
        (filled[0].cpu.unwrap() - 50.0).abs() < 1e-9,
        "the mean of the two"
    );
    assert!(
        filled[0].mem.is_none(),
        "what was not reported stays unreported"
    );
    assert!(
        points
            .windows(2)
            .all(|w| (w[1].t - w[0].t - 300.0).abs() < 1e-6),
        "evenly spaced"
    );
    assert!(points.iter().filter(|p| p.cpu.is_none()).all(Point::is_gap));

    let peaks = history
        .read("lorien", Range::Day, now, true)
        .await
        .expect("read");
    assert!(
        (peaks.iter().find_map(|p| p.cpu).unwrap() - 90.0).abs() < 1e-9,
        "the highest, not the mean"
    );
}

#[quench_test::test]
async fn the_same_minute_written_twice_keeps_the_first() {
    let Some((history, _guard)) = database().await else {
        return skipped("the_same_minute_written_twice_keeps_the_first");
    };
    let now: f64 = 1_800_000_000.0;
    let minute = (now / 60.0).floor() as i64 * 60 - 600;
    history
        .write("lorien", &bucket(minute, 10.0, 10.0))
        .await
        .expect("first");
    history
        .write("lorien", &bucket(minute, 99.0, 99.0))
        .await
        .expect("a repeat is not an error");
    let points = history
        .read("lorien", Range::Day, now, false)
        .await
        .expect("read");
    assert!((points.iter().find_map(|p| p.cpu).unwrap() - 10.0).abs() < 1e-9);
}

#[quench_test::test]
async fn pruning_drops_what_is_older_than_the_retention_and_nothing_else() {
    let Some((history, _guard)) = database().await else {
        return skipped("pruning_drops_what_is_older_than_the_retention_and_nothing_else");
    };
    let now: f64 = 1_800_000_000.0;
    let minute = (now / 60.0).floor() as i64 * 60;
    let day = 86_400;
    history
        .write("lorien", &bucket(minute - 31 * day, 1.0, 1.0))
        .await
        .expect("old");
    history
        .write("lorien", &bucket(minute - 29 * day, 2.0, 2.0))
        .await
        .expect("kept");
    history
        .write_filesystems("lorien", now - 31.0 * 86_400.0, &[fs("/", 100.0, 50.0)])
        .await
        .expect("old filesystem row");
    history
        .write_filesystems("lorien", now - 29.0 * 86_400.0, &[fs("/", 100.0, 51.0)])
        .await
        .expect("kept filesystem row");
    assert_eq!(
        history.prune(now, 30).await.expect("prune"),
        2,
        "one from each table"
    );
    let month = history
        .read("lorien", Range::Month, now, false)
        .await
        .expect("read");
    let values: Vec<f64> = month.iter().filter_map(|p| p.cpu).collect();
    assert_eq!(values, [2.0]);
    assert_eq!(month.len(), 360);
}

#[quench_test::test]
async fn a_host_with_nothing_stored_is_all_gaps() {
    let Some((history, _guard)) = database().await else {
        return skipped("a_host_with_nothing_stored_is_all_gaps");
    };
    let points = history
        .read("ghost", Range::Week, 1_800_000_000.0, false)
        .await
        .expect("read");
    assert_eq!(points.len(), 336);
    assert!(points.iter().all(Point::is_gap));
}

#[quench_test::test]
async fn the_writer_folds_live_points_into_minutes_and_stores_them() {
    let Some((reader, _guard)) = database().await else {
        return skipped("the_writer_folds_live_points_into_minutes_and_stores_them");
    };
    let url = std::env::var("GAUGE_TEST_DATABASE_URL").unwrap();
    let (tap, points) = tokio::sync::mpsc::unbounded_channel();
    let writer = History::default();
    history::spawn(&writer, points, url, 30, ForecastCell::default());

    let now: f64 = 1_800_000_000.0;
    let minute = (now / 60.0).floor() * 60.0 - 600.0;
    let point = |t: f64, cpu: f64| Point {
        cpu: Some(cpu),
        rx: Some(1.0),
        ..Point::gap(t)
    };
    for (t, cpu) in [(0.0, 10.0), (10.0, 20.0), (20.0, 30.0), (60.0, 99.0)] {
        tap.send(Note::Tick(Tick {
            host: "lorien".into(),
            point: point(minute + t, cpu),
            filesystems: Vec::new(),
        }))
        .unwrap();
    }
    // The first minute is over when the fourth point arrives; the fourth's own minute is still open.
    let mut stored = Vec::new();
    for _ in 0..50 {
        stored = reader
            .read("lorien", Range::Day, now, false)
            .await
            .unwrap()
            .into_iter()
            .filter_map(|p| p.cpu)
            .collect();
        if !stored.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        stored.len(),
        1,
        "one finished minute, the open one is not stored"
    );
    assert!((stored[0] - 20.0).abs() < 1e-9, "the mean of 10, 20, 30");
    assert!(writer.available());
}

fn fs(mount: &str, size_gib: f64, used_gib: f64) -> Filesystem {
    let gib = 1024.0 * 1024.0 * 1024.0;
    Filesystem {
        device: "/dev/sda1".into(),
        mount: Some(mount.into()),
        size: size_gib * gib,
        avail: (size_gib - used_gib) * gib,
    }
}

#[quench_test::test]
async fn a_filling_filesystem_gets_a_date_and_a_level_one_does_not() {
    let Some((history, _guard)) = database().await else {
        return skipped("a_filling_filesystem_gets_a_date_and_a_level_one_does_not");
    };
    let now: f64 = 1_800_000_000.0;
    let hour = (now / 3600.0).floor() * 3600.0;
    // Three days of hourly samples: /data gains 10 GiB a day, / stays where it is.
    for h in (0..72).rev() {
        let at = hour - f64::from(h) * 3600.0;
        let used = 100.0 + 10.0 * (72.0 - f64::from(h)) / 24.0;
        history
            .write_filesystems(
                "lorien",
                at,
                &[fs("/data", 200.0, used), fs("/", 500.0, 50.0)],
            )
            .await
            .expect("write");
    }
    let list = history.forecast(now).await.expect("forecast");
    let data = list.iter().find(|d| d.mount == "/data").expect("/data");
    assert!((data.per_day.unwrap() / (1024.0 * 1024.0 * 1024.0) - 10.0).abs() < 0.01);
    let left = data.days_left.expect("it is growing");
    assert!((left - 6.9).abs() < 0.2, "{left}");
    let root = list.iter().find(|d| d.mount == "/").expect("/");
    assert!(root.per_day.is_some() && root.days_left.is_none());
}

#[quench_test::test]
async fn the_hours_row_keeps_the_latest_values_and_young_history_has_no_forecast() {
    let Some((history, _guard)) = database().await else {
        return skipped("the_hours_row_keeps_the_latest_values_and_young_history_has_no_forecast");
    };
    let now: f64 = 1_800_000_000.0;
    history
        .write_filesystems("lorien", now, &[fs("/data", 200.0, 100.0)])
        .await
        .expect("first");
    history
        .write_filesystems("lorien", now + 60.0, &[fs("/data", 200.0, 120.0)])
        .await
        .expect("same hour");
    let list = history.forecast(now + 120.0).await.expect("forecast");
    assert_eq!(list.len(), 1, "one row for the hour");
    assert!((list[0].used / (1024.0 * 1024.0 * 1024.0) - 120.0).abs() < 1e-6);
    assert!(list[0].per_day.is_none(), "an hour of history is no trend");
}

fn alert(key: &str, detail: &str, severe: bool) -> gauge_core::Alert {
    gauge_core::Alert {
        key: key.into(),
        kind: key.split(':').next().unwrap().into(),
        detail: detail.into(),
        severe,
    }
}

#[quench_test::test]
async fn an_alert_is_logged_open_then_closed_and_newest_comes_first() {
    let Some((history, _guard)) = database().await else {
        return skipped("an_alert_is_logged_open_then_closed_and_newest_comes_first");
    };
    let now: f64 = 1_800_000_000.0;
    history
        .alert_opened(
            "lorien",
            &alert("hot:k10temp", "k10temp 95 °C", true),
            now - 7200.0,
        )
        .await
        .expect("open");
    history
        .alert_opened("nogrod", &alert("down", "refused", true), now - 600.0)
        .await
        .expect("open");
    history
        .alert_closed("lorien", "hot:k10temp", now - 3600.0)
        .await
        .expect("close");

    let log = history.alert_log(now, 30, 100).await.expect("log");
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].host, "nogrod", "newest first");
    assert!(log[0].closed.is_none(), "still going");
    assert_eq!(log[1].detail, "k10temp 95 °C");
    assert!((log[1].closed.unwrap() - (now - 3600.0)).abs() < 1e-3);
    assert!((log[1].opened - (now - 7200.0)).abs() < 1e-3);
    assert!(log[1].severe);
}

#[quench_test::test]
async fn an_open_alert_is_not_opened_twice_and_can_open_again_after_it_closes() {
    let Some((history, _guard)) = database().await else {
        return skipped("an_open_alert_is_not_opened_twice_and_can_open_again_after_it_closes");
    };
    let now: f64 = 1_800_000_000.0;
    let a = alert("oom", "1 since boot", true);
    history
        .alert_opened("lorien", &a, now - 300.0)
        .await
        .expect("open");
    history
        .alert_opened("lorien", &a, now - 200.0)
        .await
        .expect("a second open is not an error");
    assert_eq!(history.alert_log(now, 30, 100).await.unwrap().len(), 1);
    history
        .alert_closed("lorien", "oom", now - 100.0)
        .await
        .expect("close");
    history
        .alert_opened("lorien", &a, now - 50.0)
        .await
        .expect("again");
    assert_eq!(history.alert_log(now, 30, 100).await.unwrap().len(), 2);
}

#[quench_test::test]
async fn what_a_previous_run_left_open_is_closed_at_start_and_old_events_are_pruned() {
    let Some((history, _guard)) = database().await else {
        return skipped(
            "what_a_previous_run_left_open_is_closed_at_start_and_old_events_are_pruned",
        );
    };
    let now: f64 = 1_800_000_000.0;
    let day = 86_400.0;
    history
        .alert_opened("lorien", &alert("oom", "1", true), now - 2.0 * day)
        .await
        .expect("open");
    assert_eq!(history.close_dangling(now).await.expect("close"), 1);
    let log = history.alert_log(now, 30, 10).await.unwrap();
    assert!((log[0].closed.unwrap() - now).abs() < 1e-3);

    history
        .alert_opened("lorien", &alert("swapping", "x", false), now - 40.0 * day)
        .await
        .expect("old");
    history
        .alert_closed("lorien", "swapping", now - 39.0 * day)
        .await
        .expect("old close");
    assert_eq!(
        history.alert_log(now, 30, 10).await.unwrap().len(),
        1,
        "older than the retention is not shown"
    );
    assert_eq!(history.prune(now, 30).await.expect("prune"), 1);
}
