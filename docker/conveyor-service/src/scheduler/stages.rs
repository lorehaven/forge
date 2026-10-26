//! Which stages a repository's pipeline has, for the "run with parameters" form.
//!
//! Conveyor only ever sees a pipeline inside a checkout, so the list is read back from what the
//! repository's runs recorded: every planned job is stored with its stage, skipped ones included. That
//! is as current as the last run - a stage added since appears after the next run, which is why the form
//! also takes names by hand.

use crate::domain::{Job, Run, Status, Trigger};
use crate::scheduler::queue::{self, QueueError};
use quench_db::prelude::Db;
use quench_serde::Serialize;

/// How many of a repository's newest runs are looked through.
const LOOK_BACK: i64 = 30;

/// A stage of the pipeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KnownStage {
    pub name: String,
    /// Whether an ordinary run includes it, so the form starts with it ticked. False for a stage that is
    /// normally left out by its own `when` (the nightly security stage): selecting it forces it on.
    pub on_by_default: bool,
}

/// Stage names in an order that respects `needs`, ties broken by name so the list never reshuffles.
#[must_use]
pub fn ordered(jobs: &[Job]) -> Vec<String> {
    let mut needs: std::collections::BTreeMap<&str, std::collections::BTreeSet<&str>> =
        std::collections::BTreeMap::new();
    for job in jobs {
        let entry = needs.entry(job.stage.as_str()).or_default();
        entry.extend(job.needs.iter().map(String::as_str));
    }

    let mut done: Vec<&str> = Vec::new();
    while done.len() < needs.len() {
        let next = needs
            .iter()
            .filter(|(name, _)| !done.contains(name))
            // A need that is not a stage of this run (or a cycle) must not hold a stage back forever.
            .find(|(_, wanted)| {
                wanted
                    .iter()
                    .all(|n| done.contains(n) || !needs.contains_key(n))
            })
            .map(|(name, _)| *name)
            .or_else(|| needs.keys().copied().find(|name| !done.contains(name)));
        match next {
            Some(name) => done.push(name),
            None => break,
        }
    }
    done.into_iter().map(str::to_string).collect()
}

/// Whether `stage` was left out of this run by its own condition (as opposed to running, or being
/// blocked by a stage that failed - which an ordinary run would still include).
fn left_out_by_condition(jobs: &[Job], stage: &str) -> bool {
    let mut of_stage = jobs.iter().filter(|job| job.stage == stage).peekable();
    of_stage.peek().is_some()
        && of_stage.all(|job| {
            job.status == Status::Skipped
                && job
                    .error
                    .as_deref()
                    .is_some_and(|reason| reason.contains("excluded by a `when`"))
        })
}

/// The stages of a repository's pipeline, from its recent runs; empty if it has never run.
pub async fn known(db: &Db, repo_id: &str) -> Result<Vec<KnownStage>, QueueError> {
    let runs = queue::list_runs(db, Some(repo_id), LOOK_BACK).await?;

    // A run that was itself limited to some stages planned only those, so it says nothing about the rest.
    let whole: Vec<&Run> = runs.iter().filter(|run| run.stages.is_none()).collect();

    let mut layout: Option<Vec<Job>> = None;
    for run in &whole {
        let jobs = queue::list_jobs(db, &run.id).await?;
        if !jobs.is_empty() {
            layout = Some(jobs);
            break;
        }
    }
    let Some(layout) = layout else {
        return Ok(Vec::new());
    };

    // What an ordinary run does: the newest whole run that was not a scheduled one.
    let mut ordinary: Option<Vec<Job>> = None;
    for run in whole.iter().filter(|run| run.trigger != Trigger::Schedule) {
        let jobs = queue::list_jobs(db, &run.id).await?;
        if !jobs.is_empty() {
            ordinary = Some(jobs);
            break;
        }
    }

    Ok(ordered(&layout)
        .into_iter()
        .map(|name| {
            let on_by_default = ordinary
                .as_ref()
                .is_none_or(|jobs| !left_out_by_condition(jobs, &name));
            KnownStage {
                name,
                on_by_default,
            }
        })
        .collect())
}
