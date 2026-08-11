use crate::config::{Config, Job};
use crate::job_log;
use crate::rsync;
use quench_cli::prelude::{Tone, print_status};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_secs(1);

pub fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let jobs: Vec<(&Job, Duration)> = config
        .jobs
        .iter()
        .filter_map(|j| j.interval.map(|secs| (j, Duration::from_secs(secs))))
        .collect();

    if jobs.is_empty() {
        print_status(
            Tone::Warn,
            "daemon",
            "no jobs have `interval` set; nothing to watch. add `interval = <seconds>` to a job to include it.",
        );
        return Ok(());
    }

    print_status(
        Tone::Info,
        "daemon",
        &format!("watching {} job(s)", jobs.len()),
    );
    for (job, interval) in &jobs {
        println!("  {} - every {}s", job.id, interval.as_secs());
    }
    println!();

    let mut last_run: HashMap<String, Instant> = HashMap::new();

    loop {
        let now = Instant::now();
        for (job, interval) in &jobs {
            if is_due(last_run.get(&job.id).copied(), now, *interval) {
                last_run.insert(job.id.clone(), now);
                if let Err(e) = run_job(job) {
                    print_status(
                        Tone::Error,
                        "daemon",
                        &format!("job `{}` failed: {e}", job.id),
                    );
                }
            }
        }
        std::thread::sleep(TICK);
    }
}

/// Whether a job with this interval, last run at `last_run` (never, if
/// `None`), is due to run again as of `now`.
fn is_due(last_run: Option<Instant>, now: Instant, interval: Duration) -> bool {
    last_run.is_none_or(|last| now.duration_since(last) >= interval)
}

fn run_job(job: &Job) -> Result<(), Box<dyn std::error::Error>> {
    print_status(Tone::Info, "daemon", &format!("running job `{}`", job.id));
    job_log::append(&job.id, "running job");

    let has_changes = match rsync::dry_run(job) {
        Ok(has_changes) => has_changes,
        Err(e) => {
            job_log::append(&job.id, &format!("dry-run failed: {e}"));
            return Err(e);
        }
    };

    if !has_changes {
        job_log::append(&job.id, "no changes");
        return Ok(());
    }

    job_log::append(&job.id, "changes detected, syncing");
    if let Err(e) = rsync::update(job) {
        job_log::append(&job.id, &format!("sync failed: {e}"));
        return Err(e);
    }

    job_log::append(&job.id, "sync completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_due_when_never_run_before() {
        assert!(is_due(None, Instant::now(), Duration::from_secs(10)));
    }

    #[test]
    fn is_not_due_immediately_after_running() {
        let now = Instant::now();
        assert!(!is_due(Some(now), now, Duration::from_secs(10)));
    }

    #[test]
    fn is_due_once_the_interval_has_fully_elapsed() {
        let now = Instant::now();
        let last = now.checked_sub(Duration::from_secs(20)).unwrap();
        assert!(is_due(Some(last), now, Duration::from_secs(10)));
    }

    #[test]
    fn is_not_due_just_before_the_interval_elapses() {
        let now = Instant::now();
        let last = now.checked_sub(Duration::from_secs(5)).unwrap();
        assert!(!is_due(Some(last), now, Duration::from_secs(10)));
    }

    #[test]
    fn is_due_exactly_at_the_interval_boundary() {
        let now = Instant::now();
        let last = now.checked_sub(Duration::from_secs(10)).unwrap();
        assert!(is_due(Some(last), now, Duration::from_secs(10)));
    }
}
