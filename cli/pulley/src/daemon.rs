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
        poll_once(&jobs, &mut last_run, Instant::now());
        std::thread::sleep(TICK);
    }
}

/// One pass over `jobs`, running whichever are due and recording when they
/// ran - the loop body of [`run`], split out so a single pass can be tested
/// without the surrounding infinite loop or a real `sleep`.
pub fn poll_once(jobs: &[(&Job, Duration)], last_run: &mut HashMap<String, Instant>, now: Instant) {
    for (job, interval) in jobs {
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
}

/// Whether a job with this interval, last run at `last_run` (never, if
/// `None`), is due to run again as of `now`.
pub fn is_due(last_run: Option<Instant>, now: Instant, interval: Duration) -> bool {
    last_run.is_none_or(|last| now.duration_since(last) >= interval)
}

pub fn run_job(job: &Job) -> Result<(), Box<dyn std::error::Error>> {
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
