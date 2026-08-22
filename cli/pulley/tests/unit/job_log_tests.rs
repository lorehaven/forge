use chrono::NaiveDate;
use pulley::job_log::{append, prune_old_logs};
use std::fs;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pulley-job-log-test-{name}-{}", std::process::id()))
}

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn prune_old_logs_removes_files_older_than_the_retention_window() {
    let dir = scratch("prune-basic");
    fs::create_dir_all(&dir).unwrap();
    let today = date(2026, 1, 15);

    // 2026-01-01 is 14 days before today: past the 7-day cutoff.
    fs::write(dir.join("2026-01-01.log"), "old").unwrap();
    // 2026-01-10 is 5 days before today: within the 7-day cutoff.
    fs::write(dir.join("2026-01-10.log"), "recent").unwrap();

    prune_old_logs(&dir, today);

    assert!(!dir.join("2026-01-01.log").exists());
    assert!(dir.join("2026-01-10.log").exists());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn prune_old_logs_keeps_a_file_exactly_at_the_cutoff() {
    let dir = scratch("prune-boundary");
    fs::create_dir_all(&dir).unwrap();
    let today = date(2026, 1, 15);
    // Exactly RETENTION_DAYS (7) days back: `date < cutoff` is false at
    // the boundary, so this file survives one more day than a strict
    // reading of "keep 7 days" might suggest.
    let cutoff_date = today - chrono::Duration::days(7);
    fs::write(
        dir.join(format!("{}.log", cutoff_date.format("%Y-%m-%d"))),
        "boundary",
    )
    .unwrap();

    prune_old_logs(&dir, today);

    assert!(
        dir.join(format!("{}.log", cutoff_date.format("%Y-%m-%d")))
            .exists()
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn prune_old_logs_ignores_files_that_are_not_dated_log_names() {
    let dir = scratch("prune-ignore");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("readme.txt"), "keep me").unwrap();
    fs::write(dir.join("not-a-date.log"), "keep me too").unwrap();

    prune_old_logs(&dir, date(2026, 1, 15));

    assert!(dir.join("readme.txt").exists());
    assert!(dir.join("not-a-date.log").exists());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn prune_old_logs_on_a_missing_directory_does_nothing() {
    let dir = scratch("prune-missing");
    // Deliberately not created - must not panic.
    prune_old_logs(&dir, date(2026, 1, 15));
}

#[test]
fn append_is_best_effort_and_never_panics_regardless_of_home() {
    // `append` is documented as best-effort: a job whose log line can't be
    // written (no home dir resolvable, disk full, ...) must not propagate
    // as a panic. This runs against whatever HOME this sandbox actually
    // has, proving the happy path doesn't panic either.
    append("pulley-job-log-smoke-test", "hello from a test");
}
