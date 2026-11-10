use crate::config::{Config, Job};
use crate::job_log;
use crate::rsync;
use crate::source::{self, Availability};
use quench_cli::prelude::{Tone, print_status};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_secs(1);

/// How often a source that is not there is looked at again, at most. A source that comes back is synced within this, not
/// after the job's own interval, which may be long.
const RECHECK: Duration = Duration::from_secs(60);

/// Syncs one job.
pub type Runner<'a> = &'a dyn Fn(&Job) -> Result<(), Box<dyn std::error::Error>>;

/// What the daemon remembers about its jobs between passes.
#[derive(Debug, Default)]
pub struct Watch {
    /// When each job last started a sync. A job whose source was not there has not started one, so it stays due.
    pub last_run: HashMap<String, Instant>,
    /// What the last check of each job's source found. A job missing here has not been checked yet.
    pub availability: HashMap<String, Availability>,
    /// Until when a job whose source was not there is left alone.
    next_check: HashMap<String, Instant>,
}

/// Something a pass has to tell the person. The state machine returns these instead of printing them, so it can be tested
/// without a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The source stopped being there, or stopped being there in another way. Said once, not on every check.
    Unavailable { job: String, why: Availability },
    /// A source that was not there is again.
    Recovered { job: String },
    /// A sync started and failed.
    Failed { job: String, error: String },
}

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

    let mut watch = Watch::default();

    loop {
        report(&poll_once(
            &jobs,
            &mut watch,
            Instant::now(),
            &source::check,
            &run_job,
        ));
        std::thread::sleep(TICK);
    }
}

/// Says what a pass found, on the terminal and in the job's log.
pub fn report(events: &[Event]) {
    for event in events {
        match event {
            Event::Unavailable { job, why } => {
                let reason = why.reason();
                print_status(
                    Tone::Warn,
                    "daemon",
                    &format!(
                        "job `{job}`: source not available ({reason}); checking again, nothing is synced meanwhile"
                    ),
                );
                job_log::append(job, &format!("source not available: {reason}"));
            }
            Event::Recovered { job } => {
                print_status(
                    Tone::Info,
                    "daemon",
                    &format!("job `{job}`: source is available again"),
                );
                job_log::append(job, "source is available again");
            }
            Event::Failed { job, error } => {
                print_status(
                    Tone::Error,
                    "daemon",
                    &format!("job `{job}` failed: {error}"),
                );
            }
        }
    }
}

/// One pass over `jobs`: for each that is due, look at its source; sync it if the source is there, otherwise leave the
/// destination alone and look again soon.
///
/// `probe` finds out whether a source is there and `run` syncs a job; they are parameters so a pass can be tested without
/// rsync or a network. The loop body of [`run`].
pub fn poll_once(
    jobs: &[(&Job, Duration)],
    watch: &mut Watch,
    now: Instant,
    probe: &dyn Fn(&Job) -> Availability,
    run: Runner<'_>,
) -> Vec<Event> {
    let mut events = Vec::new();
    for (job, interval) in jobs {
        let id = &job.id;
        if !is_due(watch.last_run.get(id).copied(), now, *interval) {
            continue;
        }
        if watch.next_check.get(id).is_some_and(|until| now < *until) {
            continue;
        }

        let found = probe(job);
        let before = watch.availability.insert(id.clone(), found.clone());
        if !found.is_ready() {
            // Not started, so still due: it syncs as soon as the source is back, however long its interval is.
            watch
                .next_check
                .insert(id.clone(), now + RECHECK.min(*interval));
            if before.is_none_or(|before| !before.same_kind(&found)) {
                events.push(Event::Unavailable {
                    job: id.clone(),
                    why: found,
                });
            }
            continue;
        }
        watch.next_check.remove(id);
        if before.is_some_and(|before| !before.is_ready()) {
            events.push(Event::Recovered { job: id.clone() });
        }

        watch.last_run.insert(id.clone(), now);
        if let Err(error) = run(job) {
            events.push(Event::Failed {
                job: id.clone(),
                error: error.to_string(),
            });
        }
    }
    events
}

/// Whether a job with this interval, last run at `last_run` (never, if
/// `None`), is due to run again as of `now`.
#[must_use]
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
