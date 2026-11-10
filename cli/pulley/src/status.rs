//! `pulley status`: what each job is doing now and how its last runs went, read from what the daemon and `pulley run` recorded
//! (see [`crate::state`]).

use crate::config::Config;
use crate::daemon::format_duration;
use crate::state::{Live, Outcome, Run, Store, alive, now};
use quench_core::time::Time;
use quench_serde::Serialize;
use quench_tz::Zone;
use std::fmt::Write as _;
use std::time::Duration;

/// How many of its last runs a job shows in the JSON, and judges its speed by.
const RECENT: usize = 10;

/// One job's state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JobStatus {
    pub id: String,
    pub desc: String,
    /// Seconds between runs under the daemon; `None` for a job that is only run by hand.
    pub interval: Option<u64>,
    /// `running`, `interrupted` (a run was cut off) or `idle`.
    pub state: String,
    /// The run in progress, or the one that was cut off.
    pub live: Option<Live>,
    /// The last runs, oldest first.
    pub runs: Vec<Run>,
}

/// The status of the configured jobs (of `only`, when it names some), in configuration order.
#[must_use]
pub fn collect(config: &Config, store: &Store, only: &[String]) -> Vec<JobStatus> {
    config
        .jobs
        .iter()
        .filter(|job| only.is_empty() || only.contains(&job.id))
        .map(|job| {
            let live = store.live(&job.id);
            let state = match &live {
                Some(live) if alive(live.pid) => "running",
                Some(_) => "interrupted",
                None => "idle",
            };
            JobStatus {
                id: job.id.clone(),
                desc: job.desc.clone(),
                interval: job.interval,
                state: state.to_string(),
                live,
                runs: store.runs(&job.id, RECENT),
            }
        })
        .collect()
}

// ---------------------------------------------------------------- words

/// `94.2 GB`: 1024s, one decimal.
#[must_use]
pub fn bytes(count: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    #[allow(clippy::cast_precision_loss, reason = "a size for people to read")]
    let mut value = count as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{count} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn rate(per_second: u64) -> String {
    format!("{}/s", bytes(per_second))
}

fn span(secs: i64) -> String {
    format_duration(Duration::from_secs(u64::try_from(secs.max(0)).unwrap_or(0)))
}

/// The time of day of `at` on this machine's clock, with the date when it is not today (`now`).
#[must_use]
pub fn clock(at: i64, now: i64) -> String {
    let zone = Zone::local();
    let local = |secs: i64| zone.civil_at(Time::from_unix(secs)).to_time();
    match (local(at), local(now)) {
        (Some(then), Some(today)) => {
            if then.format("%Y-%m-%d") == today.format("%Y-%m-%d") {
                then.format("%H:%M")
            } else {
                then.format("%m-%d %H:%M")
            }
        }
        _ => format!("{at}"),
    }
}

// ---------------------------------------------------------------- the text

fn last_run(run: &Run, now: i64) -> String {
    let when = format!(
        "{} ({} ago)",
        clock(run.started, now),
        span(now - run.finished)
    );
    match run.outcome {
        Outcome::Synced => {
            let speed = run
                .rate()
                .map_or(String::new(), |r| format!(" at {}", rate(r)));
            format!(
                "last run {when}: copied {} in {}{speed}; scan {}; {} new, {} changed, {} deleted",
                bytes(run.bytes),
                span(i64::try_from(run.sync_secs).unwrap_or(0)),
                span(i64::try_from(run.scan_secs).unwrap_or(0)),
                run.created,
                run.modified,
                run.deleted
            )
        }
        Outcome::NoChanges => format!(
            "last run {when}: no changes (scan {})",
            span(i64::try_from(run.scan_secs).unwrap_or(0))
        ),
        Outcome::Failed => format!("last run {when}: FAILED: {}", run.error),
    }
}

/// How fast copies have been: total bytes over total seconds of the recent copies, with the fastest and slowest.
fn typical(runs: &[Run]) -> Option<String> {
    let copies: Vec<&Run> = runs.iter().filter(|r| r.rate().is_some()).collect();
    if copies.len() < 2 {
        return None;
    }
    let (total, secs) = copies
        .iter()
        .fold((0_u64, 0_u64), |(b, s), r| (b + r.bytes, s + r.sync_secs));
    let rates: Vec<u64> = copies.iter().filter_map(|r| r.rate()).collect();
    Some(format!(
        "typical copy speed {} over the last {} copies (slowest {}, fastest {})",
        rate(total / secs.max(1)),
        copies.len(),
        rate(*rates.iter().min()?),
        rate(*rates.iter().max()?)
    ))
}

fn running(live: &Live, now: i64) -> Vec<String> {
    let mut lines = vec![format!(
        "running for {}: {}",
        span(now - live.started),
        if live.phase == "scanning" {
            "looking for what changed (the dry run)".to_string()
        } else {
            format!("copying for {}", span(now - live.phase_started))
        }
    )];
    if live.phase == "syncing" {
        let elapsed = (now - live.phase_started).max(1);
        let average = live.bytes / u64::try_from(elapsed).unwrap_or(1);
        lines.push(format!(
            "copied {} so far: {} on average, {} right now, {}% of what rsync has found yet",
            bytes(live.bytes),
            rate(average),
            rate(live.rate),
            live.percent
        ));
    }
    lines
}

/// The text of `pulley status`.
#[must_use]
pub fn render(statuses: &[JobStatus], now: i64) -> String {
    let mut out = String::new();
    for status in statuses {
        let _ = writeln!(out, "{}  {}", status.id, status.desc);
        match (status.state.as_str(), &status.live) {
            ("running", Some(live)) => {
                for line in running(live, now) {
                    let _ = writeln!(out, "  {line}");
                }
            }
            ("interrupted", Some(live)) => {
                let _ = writeln!(
                    out,
                    "  interrupted: pulley stopped while {} (last seen {} ago)",
                    if live.phase == "scanning" {
                        "scanning"
                    } else {
                        "copying"
                    },
                    span(now - live.updated)
                );
            }
            _ => {}
        }
        if let Some(run) = status.runs.last() {
            let _ = writeln!(out, "  {}", last_run(run, now));
        } else {
            let _ = writeln!(out, "  no run recorded yet");
        }
        if let Some(line) = typical(&status.runs) {
            let _ = writeln!(out, "  {line}");
        }
        if status.state != "running"
            && let (Some(interval), Some(run)) = (status.interval, status.runs.last())
        {
            let due = run.started + i64::try_from(interval).unwrap_or(i64::MAX);
            if due <= now {
                let _ = writeln!(out, "  next run: due now");
            } else {
                let _ = writeln!(out, "  next run: in {}", span(due - now));
            }
        }
        out.push('\n');
    }
    out
}

/// `pulley status [JOB...] [--json]`.
///
/// # Errors
/// There is no home directory to keep state in, or no job matches.
pub fn command(only: &[String], json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load_merged_quietly()?;
    let store = Store::global().ok_or("there is no home directory to keep job state in")?;
    let statuses = collect(&config, &store, only);
    if statuses.is_empty() {
        return Err("no job matches".into());
    }
    if json {
        println!("{}", quench_serde::json::to_string_pretty(&statuses));
    } else {
        print!("{}", render(&statuses, now()));
    }
    Ok(())
}
