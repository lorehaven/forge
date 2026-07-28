//! Reading `.conveyor.toml`.
//!
//! Every check happens here, before anything is executed. A pipeline that
//! cannot be run is a parse error naming the stage and job at fault, not a run
//! that gets three stages in and then discovers a typo in the fourth - by which
//! point it has already pushed an image.
//!
//! The steps array is converted by hand rather than through a serde enum. TOML
//! spells a step `{ anvil = "build --all" }`, and serde's message for a table
//! that does not match any variant says nothing about which step, in which job,
//! or what the alternatives were.

use crate::pipeline::condition::{Condition, ConditionError};
use crate::pipeline::graph::{self, GraphError};
use crate::pipeline::spec::{Job, PipelineSpec, Stage, Step, Triggers};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("{0}")]
    Toml(#[from] toml::de::Error),

    #[error("pipeline has no stages: add a [[stage]]")]
    NoStages,

    #[error("stage {position} has an empty name")]
    EmptyStageName { position: usize },

    #[error("stage '{stage}' has no jobs: add a [[stage.job]]")]
    NoJobs { stage: String },

    #[error("a job in stage '{stage}' has an empty name")]
    EmptyJobName { stage: String },

    #[error("duplicate job '{name}' in stage '{stage}'")]
    DuplicateJob { stage: String, name: String },

    #[error("job '{job}' in stage '{stage}' has no steps")]
    NoSteps { stage: String, job: String },

    #[error(
        "job '{job}' in stage '{stage}' declares `needs`; \
         dependencies are between stages, so move it to the [[stage]]"
    )]
    JobNeeds { stage: String, job: String },

    #[error("step {ordinal} of job '{job}' in stage '{stage}': {reason}")]
    BadStep {
        stage: String,
        job: String,
        ordinal: usize,
        reason: String,
    },

    #[error("`when` on stage '{stage}': {source}")]
    BadStageCondition {
        stage: String,
        #[source]
        source: ConditionError,
    },

    #[error("`when` on job '{job}' in stage '{stage}': {source}")]
    BadJobCondition {
        stage: String,
        job: String,
        #[source]
        source: ConditionError,
    },

    #[error("job '{job}' in stage '{stage}' has a timeout of zero")]
    ZeroTimeout { stage: String, job: String },

    #[error("job '{job}' in stage '{stage}' lists an empty secret name")]
    EmptySecretName { stage: String, job: String },

    #[error("`on.{event}` contains an empty pattern; use \"*\" to match every ref")]
    EmptyTriggerPattern { event: String },

    #[error("{0}")]
    Graph(#[from] GraphError),
}

/// Parses and fully validates a `.conveyor.toml`.
pub fn parse(source: &str) -> Result<PipelineSpec, SpecError> {
    let raw: RawPipeline = toml::from_str(source)?;

    if raw.stages.is_empty() {
        return Err(SpecError::NoStages);
    }

    let on = raw
        .on
        .map_or_else(|| Ok(Triggers::default()), parse_triggers)?;

    let mut stages = Vec::with_capacity(raw.stages.len());
    for (position, raw_stage) in raw.stages.into_iter().enumerate() {
        stages.push(parse_stage(position, raw_stage)?);
    }

    // Ordering is checked last so that a name or step mistake is reported in
    // preference to the graph error it probably caused.
    let order = graph::topological_order(&stages)?;

    Ok(PipelineSpec::new(on, stages, order))
}

/// An `on` table replaces the default outright: an event it does not name
/// simply does not trigger.
///
/// The alternative - merging with [`Triggers::default`] - would mean
/// `on = { pull_request = ["*"] }` still built every push, which is not what it
/// looks like it says. Omitting `on` altogether is the way to get the default.
fn parse_triggers(raw: RawTriggers) -> Result<Triggers, SpecError> {
    let resolve = |patterns: Option<Vec<String>>, event: &str| {
        let Some(patterns) = patterns else {
            return Ok(Vec::new());
        };
        if patterns.iter().any(|pattern| pattern.trim().is_empty()) {
            return Err(SpecError::EmptyTriggerPattern {
                event: event.to_string(),
            });
        }
        Ok(patterns)
    };

    Ok(Triggers {
        push: resolve(raw.push, "push")?,
        pull_request: resolve(raw.pull_request, "pull_request")?,
        tag: resolve(raw.tag, "tag")?,
    })
}

fn parse_stage(position: usize, raw: RawStage) -> Result<Stage, SpecError> {
    let name = raw.name.trim().to_string();
    if name.is_empty() {
        return Err(SpecError::EmptyStageName { position });
    }

    if raw.jobs.is_empty() {
        return Err(SpecError::NoJobs { stage: name });
    }

    let when = raw
        .when
        .as_deref()
        .map(Condition::parse)
        .transpose()
        .map_err(|source| SpecError::BadStageCondition {
            stage: name.clone(),
            source,
        })?;

    let only_job = raw.jobs.len() == 1;
    let mut jobs: Vec<Job> = Vec::with_capacity(raw.jobs.len());
    for (index, raw_job) in raw.jobs.into_iter().enumerate() {
        let parsed = parse_job(&name, index, only_job, raw_job)?;
        if jobs.iter().any(|existing| existing.name == parsed.name) {
            return Err(SpecError::DuplicateJob {
                stage: name,
                name: parsed.name,
            });
        }
        jobs.push(parsed);
    }

    Ok(Stage {
        name,
        needs: raw.needs,
        when,
        jobs,
    })
}

fn parse_job(stage: &str, index: usize, only_job: bool, raw: RawJob) -> Result<Job, SpecError> {
    // An unnamed sole job takes the stage's name, which is what "build/build"
    // would have said anyway. Where there are several, they are numbered, and
    // an author who wants them distinguishable in a report names them.
    let name = match raw.name {
        Some(given) => {
            let trimmed = given.trim().to_string();
            if trimmed.is_empty() {
                return Err(SpecError::EmptyJobName {
                    stage: stage.to_string(),
                });
            }
            trimmed
        }
        None if only_job => stage.to_string(),
        None => format!("job-{}", index + 1),
    };

    if raw.needs.is_some() {
        return Err(SpecError::JobNeeds {
            stage: stage.to_string(),
            job: name,
        });
    }

    if raw.steps.is_empty() {
        return Err(SpecError::NoSteps {
            stage: stage.to_string(),
            job: name,
        });
    }

    let when = raw
        .when
        .as_deref()
        .map(Condition::parse)
        .transpose()
        .map_err(|source| SpecError::BadJobCondition {
            stage: stage.to_string(),
            job: name.clone(),
            source,
        })?;

    if raw.timeout == Some(0) {
        return Err(SpecError::ZeroTimeout {
            stage: stage.to_string(),
            job: name,
        });
    }

    if raw.secrets.iter().any(|secret| secret.trim().is_empty()) {
        return Err(SpecError::EmptySecretName {
            stage: stage.to_string(),
            job: name,
        });
    }

    let mut steps = Vec::with_capacity(raw.steps.len());
    for (ordinal, value) in raw.steps.into_iter().enumerate() {
        steps.push(parse_step(value).map_err(|reason| SpecError::BadStep {
            stage: stage.to_string(),
            job: name.clone(),
            ordinal: ordinal + 1,
            reason,
        })?);
    }

    Ok(Job {
        name,
        when,
        env: raw.env,
        secrets: raw.secrets,
        timeout: raw.timeout,
        image: raw.image,
        artifacts: raw.artifacts,
        steps,
    })
}

/// Converts one entry of a `steps` array, returning the reason it could not be.
fn parse_step(value: toml::Value) -> Result<Step, String> {
    let table = match value {
        // The shorthand: a bare string is a shell command, because that is what
        // it would have been in any case.
        toml::Value::String(command) => return parse_command_step("run", command),
        toml::Value::Table(table) => table,
        other => {
            return Err(format!(
                "expected a command string or a table like {{ run = \"...\" }}, found {}",
                other.type_str()
            ));
        }
    };

    let mut entries = table.into_iter();
    let Some((kind, command)) = entries.next() else {
        return Err("step is empty".to_string());
    };

    if let Some((second, _)) = entries.next() {
        let mut named = vec![kind, second];
        named.extend(entries.map(|(key, _)| key));
        named.sort();
        return Err(format!(
            "a step names exactly one tool, found {}",
            named.join(", ")
        ));
    }

    let toml::Value::String(command) = command else {
        return Err(format!("`{kind}` expects a command string"));
    };

    parse_command_step(&kind, command)
}

fn parse_command_step(kind: &str, command: String) -> Result<Step, String> {
    if command.trim().is_empty() {
        return Err(format!("`{kind}` has an empty command"));
    }

    let step = Step::new(kind, command).ok_or_else(|| {
        format!(
            "unknown step kind `{kind}` (known: {})",
            Step::KINDS.join(", ")
        )
    })?;

    // Checked here rather than when the step is spawned. By then the checkout
    // has happened and earlier stages have already run, and the author learns
    // about a typo in their deploy command from a failed deploy.
    crate::steps::validate(&step).map_err(|error| error.to_string())?;

    Ok(step)
}

// ---------------------------------------------------------------------------
// The shape on disk
// ---------------------------------------------------------------------------
//
// `deny_unknown_fields` throughout: a misspelled key that is silently ignored
// is a pipeline that quietly does not do what it says.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPipeline {
    #[serde(default)]
    on: Option<RawTriggers>,
    #[serde(default, rename = "stage")]
    stages: Vec<RawStage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTriggers {
    #[serde(default)]
    push: Option<Vec<String>>,
    #[serde(default)]
    pull_request: Option<Vec<String>>,
    #[serde(default)]
    tag: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStage {
    name: String,
    #[serde(default)]
    needs: Vec<String>,
    #[serde(default)]
    when: Option<String>,
    #[serde(default, rename = "job")]
    jobs: Vec<RawJob>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawJob {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    when: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    secrets: Vec<String>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    artifacts: Vec<String>,
    #[serde(default)]
    steps: Vec<toml::Value>,

    /// Accepted only so it can be rejected with an explanation. Left to
    /// `deny_unknown_fields`, a `needs` on a job reads as a typo rather than as
    /// a key that belongs one level up.
    #[serde(default)]
    needs: Option<Vec<String>>,
}
