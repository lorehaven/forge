use crate::config::{Config, Job};
use crate::job_log;
use crate::rsync::{self, Output};
use crate::source::{self, Availability};
use quench_cli::prelude::{Tone, print_status};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_secs(1);

/// How often a source that is not there is looked at again, at most. A source that comes back is synced within this, not
/// after the job's own interval, which may be long.
const RECHECK: Duration = Duration::from_secs(60);

/// Syncs one job.
pub type Runner<'a> = &'a dyn Fn(&Job) -> Result<(), Box<dyn std::error::Error>>;

/// A [`Runner`] that several workers may call at once.
pub type SharedRunner<'a> = &'a (dyn Fn(&Job) -> Result<(), Box<dyn std::error::Error>> + Sync);

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
    let jobs: Vec<(Job, Duration)> = config
        .jobs
        .iter()
        .filter_map(|j| {
            j.interval
                .map(|secs| (j.clone(), Duration::from_secs(secs)))
        })
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
    let groups = group_by_dest(&jobs);
    for group in groups.iter().filter(|group| group.len() > 1) {
        let ids: Vec<&str> = group.iter().map(|(job, _)| job.id.as_str()).collect();
        println!(
            "  {} share a destination, so they run one after another",
            ids.join(", ")
        );
    }
    println!();

    serve(
        &groups,
        &AtomicBool::new(false),
        TICK,
        &source::check,
        &run_job,
    )
}

/// The jobs split into groups that may run at the same time: one group per destination.
///
/// Jobs that write to the same destination must not, so they share a group, in the order given; every other job has its
/// own, so a copy that takes hours does not hold up the rest.
#[must_use]
pub fn group_by_dest(jobs: &[(Job, Duration)]) -> Vec<Vec<(Job, Duration)>> {
    let mut groups: Vec<(String, Vec<(Job, Duration)>)> = Vec::new();
    for (job, interval) in jobs {
        let key = job.dest.trim_end_matches('/').to_string();
        match groups.iter_mut().find(|(found, _)| *found == key) {
            Some((_, group)) => group.push((job.clone(), *interval)),
            None => groups.push((key, vec![(job.clone(), *interval)])),
        }
    }
    groups.into_iter().map(|(_, group)| group).collect()
}

/// Watches every group, each on a thread of its own, until `stop` is set (never, for the daemon).
///
/// `probe` finds out whether a source is there and `run` syncs a job; both are called from several threads at once.
///
/// # Errors
/// If a worker died (it panicked): the others are stopped too, so a service manager restarts the whole daemon instead of
/// leaving some jobs silently unwatched.
pub fn serve(
    groups: &[Vec<(Job, Duration)>],
    stop: &AtomicBool,
    tick: Duration,
    probe: &(dyn Fn(&Job) -> Availability + Sync),
    run: SharedRunner<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let crashed = AtomicBool::new(false);
    std::thread::scope(|scope| {
        for group in groups {
            let crashed = &crashed;
            scope.spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    watch_group(group, stop, tick, probe, run);
                }));
                if outcome.is_err() {
                    print_status(
                        Tone::Error,
                        "daemon",
                        "a worker stopped unexpectedly; stopping so it can be restarted",
                    );
                    crashed.store(true, Ordering::SeqCst);
                    stop.store(true, Ordering::SeqCst);
                }
            });
        }
    });
    if crashed.load(Ordering::SeqCst) {
        Err("a worker stopped unexpectedly".into())
    } else {
        Ok(())
    }
}

/// The loop of one worker: its jobs, passed over once a tick.
fn watch_group(
    group: &[(Job, Duration)],
    stop: &AtomicBool,
    tick: Duration,
    probe: &(dyn Fn(&Job) -> Availability + Sync),
    run: SharedRunner<'_>,
) {
    let jobs: Vec<(&Job, Duration)> = group
        .iter()
        .map(|(job, interval)| (job, *interval))
        .collect();
    let mut watch = Watch::default();
    while !stop.load(Ordering::SeqCst) {
        report(&poll_once(&jobs, &mut watch, Instant::now(), probe, run));
        std::thread::sleep(tick);
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
            // A host that went away mid-sync fails the sync. If that is why, it is not a failed job but a source that is not
            // there: the job stays due and is looked at again in a minute, so it resumes as soon as the host is back, not
            // after its whole interval.
            let after = probe(job);
            if after.is_ready() {
                events.push(Event::Failed {
                    job: id.clone(),
                    error: error.to_string(),
                });
            } else {
                // Work is left undone, so it is due again as soon as the source is back, whatever the last run was.
                watch.last_run.remove(id);
                watch.availability.insert(id.clone(), after.clone());
                watch
                    .next_check
                    .insert(id.clone(), now + RECHECK.min(*interval));
                events.push(Event::Unavailable {
                    job: id.clone(),
                    why: after,
                });
            }
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

/// Syncs one job as the daemon does: no line per file and no progress, only what happened, in the job's log and as one line
/// on the terminal (and so in the journal).
pub fn run_job(job: &Job) -> Result<(), Box<dyn std::error::Error>> {
    print_status(Tone::Info, "daemon", &format!("running job `{}`", job.id));
    job_log::append(&job.id, "running job");

    let changes = match rsync::scan(job, Output::Summary) {
        Ok(changes) => changes,
        Err(e) => {
            job_log::append(&job.id, &format!("dry-run failed: {e}"));
            return Err(e);
        }
    };

    if changes.total() == 0 {
        job_log::append(&job.id, "no changes");
        return Ok(());
    }

    let what = changes.describe();
    job_log::append(&job.id, &format!("changes detected ({what}), syncing"));
    let started = Instant::now();
    if let Err(e) = rsync::update_quietly(job) {
        job_log::append(&job.id, &format!("sync failed: {e}"));
        return Err(e);
    }

    let took = format_duration(started.elapsed());
    job_log::append(&job.id, &format!("sync completed in {took}"));
    print_status(
        Tone::Success,
        "daemon",
        &format!("job `{}`: {what}; synced in {took}", job.id),
    );
    Ok(())
}

/// `45s`, `4m12s`, `3h05m`: whole seconds, and only the units that matter.
#[must_use]
pub fn format_duration(took: Duration) -> String {
    let secs = took.as_secs();
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m{:02}s", secs / 60, secs % 60),
        _ => format!("{}h{:02}m", secs / 3600, secs % 3600 / 60),
    }
}
