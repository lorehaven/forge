use crate::config::Config;
use chrono::{Duration, Local, NaiveDate};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// How many days of daily log files to keep per job before pruning older
/// ones, so an unattended daemon can't slowly fill the disk.
const RETENTION_DAYS: i64 = 7;

fn job_log_dir(job_id: &str) -> Option<PathBuf> {
    let dir = Config::global_config_dir()?.join("logs").join(job_id);
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Best-effort: a job whose log line can't be written (no home dir, disk
/// full, ...) shouldn't stop the sync itself.
pub fn append(job_id: &str, message: &str) {
    let Some(dir) = job_log_dir(job_id) else {
        return;
    };

    let now = Local::now();
    let path = dir.join(format!("{}.log", now.format("%Y-%m-%d")));
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "[{}] {message}", now.format("%H:%M:%S"));
    }

    prune_old_logs(&dir, now.date_naive());
}

fn prune_old_logs(dir: &Path, today: NaiveDate) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let cutoff = today - Duration::days(RETENTION_DAYS);

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let is_old = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|stem| NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok())
            .is_some_and(|date| date < cutoff);
        if is_old {
            let _ = fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let cutoff_date = today - Duration::days(RETENTION_DAYS);
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
}
