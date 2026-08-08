use crate::config::{Config, Job};
use crate::job_log;
use crate::rsync;
use quench_cli::prelude::{Tone, print_status};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_secs(1);

pub fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let jobs: Vec<&Job> = config
        .jobs
        .iter()
        .filter(|j| j.interval.is_some())
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
    for job in &jobs {
        println!("  {} - every {}s", job.id, job.interval.unwrap());
    }
    println!();

    let mut last_run: HashMap<String, Instant> = HashMap::new();

    loop {
        let now = Instant::now();
        for job in &jobs {
            let interval = Duration::from_secs(job.interval.unwrap());
            let due = last_run
                .get(&job.id)
                .map(|last| now.duration_since(*last) >= interval)
                .unwrap_or(true);

            if due {
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
