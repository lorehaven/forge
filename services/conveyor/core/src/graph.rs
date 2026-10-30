//! A run's jobs laid out the way the worker runs them: one row per dependency level.

use crate::domain::Job;
use std::collections::{HashMap, HashSet};

/// Level *n* needs only the levels below it: the stages of a level run concurrently. Each level holds its stages, each stage its
/// jobs. Grouped from the `Job` rows, not the pipeline file, so an old run renders the same.
#[must_use]
pub fn stage_levels(jobs: &[Job]) -> Vec<Vec<Vec<&Job>>> {
    let mut stage_order: Vec<&str> = Vec::new();
    let mut jobs_by_stage: HashMap<&str, Vec<&Job>> = HashMap::new();
    let mut needs_by_stage: HashMap<&str, &[String]> = HashMap::new();

    for job in jobs {
        if !jobs_by_stage.contains_key(job.stage.as_str()) {
            stage_order.push(job.stage.as_str());
        }
        jobs_by_stage
            .entry(job.stage.as_str())
            .or_default()
            .push(job);
        needs_by_stage
            .entry(job.stage.as_str())
            .or_insert(job.needs.as_slice());
    }

    let mut level_of: HashMap<&str, usize> = HashMap::new();
    let mut visiting: HashSet<&str> = HashSet::new();
    for &stage in &stage_order {
        stage_level(stage, &needs_by_stage, &mut level_of, &mut visiting);
    }

    let level_count = level_of.values().copied().max().map_or(0, |max| max + 1);
    let mut levels: Vec<Vec<Vec<&Job>>> = vec![Vec::new(); level_count.max(1)];
    for &stage in &stage_order {
        levels[level_of[stage]].push(jobs_by_stage.remove(stage).unwrap_or_default());
    }
    levels.retain(|level| !level.is_empty());
    levels
}

/// `visiting` guards a `needs` cycle: the parser already refuses one, so this is only a backstop for a hand-edited database.
fn stage_level<'a>(
    stage: &'a str,
    needs_by_stage: &HashMap<&'a str, &'a [String]>,
    level_of: &mut HashMap<&'a str, usize>,
    visiting: &mut HashSet<&'a str>,
) -> usize {
    if let Some(&level) = level_of.get(stage) {
        return level;
    }
    if !visiting.insert(stage) {
        return 0;
    }

    let needs = needs_by_stage.get(stage).copied().unwrap_or(&[]);
    let level = needs
        .iter()
        .filter(|need| needs_by_stage.contains_key(need.as_str()))
        .map(|need| stage_level(need, needs_by_stage, level_of, visiting) + 1)
        .max()
        .unwrap_or(0);

    visiting.remove(stage);
    level_of.insert(stage, level);
    level
}
