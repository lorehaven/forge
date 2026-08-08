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
