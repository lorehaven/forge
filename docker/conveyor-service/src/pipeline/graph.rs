//! The stage graph: ordering, and deciding what a given run will actually do.
//!
//! Two separate jobs, kept apart on purpose. [`topological_order`] is a
//! property of the pipeline alone and is checked once, at parse time, so an
//! invalid graph is a parse error rather than a run that fails halfway.
//! [`plan`] is a property of the pipeline *and* a particular run, and answers
//! which stages that run will execute.

use crate::pipeline::condition::EvalContext;
use crate::pipeline::spec::{PipelineSpec, Stage};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GraphError {
    #[error("duplicate stage '{name}'")]
    DuplicateStage { name: String },

    #[error("stage '{stage}' needs '{missing}', which is not a stage in this pipeline")]
    UnknownNeeds { stage: String, missing: String },

    #[error("stage '{stage}' needs itself")]
    SelfNeeds { stage: String },

    #[error("stages form a cycle: {}", .path.join(" -> "))]
    Cycle { path: Vec<String> },
}

/// Orders stages so that a stage always follows everything it needs, and
/// rejects a graph that cannot be ordered.
///
/// Ties are broken by declaration order, so the same pipeline always produces
/// the same plan - a run report that reshuffles between runs is one nobody can
/// compare against the last one.
pub fn topological_order(stages: &[Stage]) -> Result<Vec<usize>, GraphError> {
    let mut index_of: HashMap<&str, usize> = HashMap::new();
    for (index, stage) in stages.iter().enumerate() {
        if index_of.insert(stage.name.as_str(), index).is_some() {
            return Err(GraphError::DuplicateStage {
                name: stage.name.clone(),
            });
        }
    }

    // Resolve `needs` to indices up front, so ordering deals only in numbers
    // and every name error is reported before any of it.
    let mut dependencies: Vec<Vec<usize>> = Vec::with_capacity(stages.len());
    for stage in stages {
        let mut resolved = Vec::with_capacity(stage.needs.len());
        for need in &stage.needs {
            let Some(&target) = index_of.get(need.as_str()) else {
                return Err(GraphError::UnknownNeeds {
                    stage: stage.name.clone(),
                    missing: need.clone(),
                });
            };
            if stages[target].name == stage.name {
                return Err(GraphError::SelfNeeds {
                    stage: stage.name.clone(),
                });
            }
            resolved.push(target);
        }
        dependencies.push(resolved);
    }

    let mut remaining: Vec<usize> = dependencies.iter().map(Vec::len).collect();
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); stages.len()];
    for (index, needs) in dependencies.iter().enumerate() {
        for &need in needs {
            dependents[need].push(index);
        }
    }

    let mut order = Vec::with_capacity(stages.len());
    let mut ready: Vec<usize> = (0..stages.len()).filter(|&i| remaining[i] == 0).collect();

    while let Some(index) = pop_lowest(&mut ready) {
        order.push(index);
        for &dependent in &dependents[index] {
            remaining[dependent] -= 1;
            if remaining[dependent] == 0 {
                ready.push(dependent);
            }
        }
    }

    if order.len() == stages.len() {
        return Ok(order);
    }

    // Whatever is left is in, or downstream of, a cycle. Naming the cycle beats
    // "there is a cycle somewhere": the author has to find it either way, and
    // only one of us is holding the graph.
    Err(GraphError::Cycle {
        path: find_cycle(stages, &dependencies),
    })
}

/// Takes the lowest index from `ready`, keeping the output in declaration order
/// wherever the graph leaves a choice.
fn pop_lowest(ready: &mut Vec<usize>) -> Option<usize> {
    let position = ready.iter().enumerate().min_by_key(|&(_, &index)| index)?.0;
    Some(ready.remove(position))
}

/// Walks the graph until it re-enters a stage still on the current path; that
/// stage and everything after it on the path is the cycle.
fn find_cycle(stages: &[Stage], dependencies: &[Vec<usize>]) -> Vec<String> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Unvisited,
        InProgress,
        Done,
    }

    fn walk(
        index: usize,
        dependencies: &[Vec<usize>],
        marks: &mut Vec<Mark>,
        path: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        marks[index] = Mark::InProgress;
        path.push(index);

        for &need in &dependencies[index] {
            match marks[need] {
                Mark::InProgress => {
                    let start = path.iter().position(|&i| i == need).unwrap_or(0);
                    let mut cycle = path[start..].to_vec();
                    cycle.push(need);
                    return Some(cycle);
                }
                Mark::Unvisited => {
                    if let Some(cycle) = walk(need, dependencies, marks, path) {
                        return Some(cycle);
                    }
                }
                Mark::Done => {}
            }
        }

        path.pop();
        marks[index] = Mark::Done;
        None
    }

    let mut marks = vec![Mark::Unvisited; stages.len()];
    for index in 0..stages.len() {
        if marks[index] != Mark::Unvisited {
            continue;
        }
        let mut path = Vec::new();
        if let Some(cycle) = walk(index, dependencies, &mut marks, &mut path) {
            return cycle.into_iter().map(|i| stages[i].name.clone()).collect();
        }
    }

    Vec::new()
}

// ---------------------------------------------------------------------------
// Planning a particular run
// ---------------------------------------------------------------------------

/// Why a stage or job is, or is not, going to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Run,
    /// A `when` on this stage or job evaluated false.
    Excluded,
    /// Something this depends on is not running.
    Blocked {
        by: String,
    },
}

impl Decision {
    pub const fn will_run(&self) -> bool {
        matches!(self, Self::Run)
    }

    /// How this reads in a run report.
    pub fn reason(&self) -> Option<String> {
        match self {
            Self::Run => None,
            Self::Excluded => Some("excluded by a `when` condition".to_string()),
            Self::Blocked { by } => Some(format!("stage '{by}' did not run")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobPlan {
    /// Index into the stage's `jobs`.
    pub index: usize,
    pub decision: Decision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagePlan {
    /// Index into the spec's `stages`.
    pub index: usize,
    pub decision: Decision,
    pub jobs: Vec<JobPlan>,
}

/// What this run will execute, in execution order.
///
/// Every stage and job appears, running or not: a report that silently omits
/// the skipped ones cannot answer "why did this not deploy", which is the
/// question conditions generate.
pub fn plan(spec: &PipelineSpec, context: &EvalContext) -> Vec<StagePlan> {
    let mut decisions: HashMap<&str, Decision> = HashMap::new();
    let mut plans = Vec::with_capacity(spec.stages.len());

    for &index in spec.order() {
        let stage = &spec.stages[index];

        // Exclusion propagates: a stage that needs a stage which did not run
        // cannot run either, however its own condition reads.
        let blocked = stage.needs.iter().find(|need| {
            decisions
                .get(need.as_str())
                .is_some_and(|decision| !decision.will_run())
        });

        let decision = match blocked {
            Some(by) => Decision::Blocked { by: by.clone() },
            None => match &stage.when {
                Some(condition) if !condition.evaluate(context) => Decision::Excluded,
                _ => Decision::Run,
            },
        };

        let jobs = stage
            .jobs
            .iter()
            .enumerate()
            .map(|(job_index, job)| JobPlan {
                index: job_index,
                decision: if decision.will_run() {
                    match &job.when {
                        Some(condition) if !condition.evaluate(context) => Decision::Excluded,
                        _ => Decision::Run,
                    }
                } else {
                    // The stage's decision verbatim, rather than
                    // `Blocked { by: <this stage> }`. A job told that its own
                    // stage did not run learns nothing; inheriting keeps the
                    // reason pointing at whatever actually caused it - the
                    // `when` that excluded the stage, or the stage upstream
                    // that failed.
                    decision.clone()
                },
            })
            .collect();

        decisions.insert(stage.name.as_str(), decision.clone());
        plans.push(StagePlan {
            index,
            decision,
            jobs,
        });
    }

    plans
}
