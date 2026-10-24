use crate::config::Config;
use quench_core::date::Date;
use quench_core::time::Time;
use quench_tz::Zone;
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

    // The machine's local time: the fields are read back through `Time::format`, which only reads them.
    let local = Zone::local().civil_at(Time::now());
    let Some(wall) = local.to_time() else {
        return;
    };
    let path = dir.join(format!("{}.log", wall.format("%Y-%m-%d")));
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "[{}] {message}", wall.format("%H:%M:%S"));
    }

    prune_old_logs(&dir, local.date());
}

pub fn prune_old_logs(dir: &Path, today: Date) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let Some(cutoff) = today.add_days(-RETENTION_DAYS) else {
        return;
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let is_old = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|stem| Date::parse_from_str(stem, "%Y-%m-%d").ok())
            .is_some_and(|date| date < cutoff);
        if is_old {
            let _ = fs::remove_file(&path);
        }
    }
}
