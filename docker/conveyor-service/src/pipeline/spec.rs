//! What a `.conveyor.toml` says, once it has been read and checked.
//!
//! These are the validated types. Everything here is already known to be
//! consistent - stage names are unique, every `needs` names a stage that
//! exists, and the graph is acyclic - because [`super::parse`] is the only way
//! to build a [`PipelineSpec`] and it refuses to return an invalid one.

use crate::pipeline::condition::Condition;
use std::collections::BTreeMap;

/// The file conveyor looks for in a checkout.
pub const PIPELINE_FILE: &str = ".conveyor.toml";

#[derive(Clone, Debug)]
pub struct PipelineSpec {
    pub on: Triggers,
    /// Stages in declaration order, which is the order they are shown in.
    pub stages: Vec<Stage>,
    /// Indices into `stages`, in an order that satisfies every `needs`.
    order: Vec<usize>,
}

impl PipelineSpec {
    pub(super) fn new(on: Triggers, stages: Vec<Stage>, order: Vec<usize>) -> Self {
        Self { on, stages, order }
    }

    /// Stage indices in an order where a stage always follows what it needs.
    pub fn order(&self) -> &[usize] {
        &self.order
    }

    /// Stages in execution order.
    pub fn stages_in_order(&self) -> impl Iterator<Item = &Stage> {
        self.order.iter().map(|&index| &self.stages[index])
    }

    pub fn stage(&self, name: &str) -> Option<&Stage> {
        self.stages.iter().find(|stage| stage.name == name)
    }

    pub fn job_count(&self) -> usize {
        self.stages.iter().map(|stage| stage.jobs.len()).sum()
    }
}

/// Which events this pipeline wants to be run for.
///
/// Each field is a list of ref patterns; an empty list means the event never
/// triggers this pipeline. Patterns are globs over the *bare* ref name, so
/// `master` and `release/*` rather than `refs/heads/master`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Triggers {
    pub push: Vec<String>,
    pub pull_request: Vec<String>,
    /// Matched against the tag name when a push carries `refs/tags/...`.
    pub tag: Vec<String>,
}

impl Default for Triggers {
    /// A pipeline that does not say when to run gets built on every push.
    ///
    /// The alternative - running nothing - makes an author who added the file
    /// and pushed it wait for a build that is never coming, with no error to
    /// explain why.
    fn default() -> Self {
        Self {
            push: vec!["*".to_string()],
            pull_request: Vec::new(),
            tag: Vec::new(),
        }
    }
}

impl Triggers {
    /// Whether this pipeline should run for `event` at `git_ref`.
    ///
    /// `git_ref` is accepted full or bare. A manual trigger is always allowed:
    /// somebody asked for it by name, and refusing would leave them no way to
    /// run a pipeline whose patterns do not cover the branch they are on.
    pub fn allows(&self, event: &str, git_ref: &str) -> bool {
        if event == "manual" {
            return true;
        }

        if let Some(tag) = git_ref.strip_prefix("refs/tags/") {
            // A tag push is only a tag push. Falling back to the `push`
            // patterns here would make `push = ["*"]` fire on every release
            // tag as well as every branch.
            return event == "push" && matches_any(&self.tag, tag);
        }

        let branch = git_ref.strip_prefix("refs/heads/").unwrap_or(git_ref);
        match event {
            "push" => matches_any(&self.push, branch),
            "pull_request" => matches_any(&self.pull_request, branch),
            _ => false,
        }
    }
}

fn matches_any(patterns: &[String], value: &str) -> bool {
    patterns.iter().any(|pattern| glob_match(pattern, value))
}

/// Glob matching over a ref name: `*` stands for any run of characters,
/// including none and including `/`.
///
/// `/` is deliberately not special. `release/*` should match `release/1.2`, and
/// a pattern language where it does not is a pattern language people get wrong.
pub fn glob_match(pattern: &str, value: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let value: Vec<char> = value.chars().collect();

    let (mut p, mut v) = (0, 0);
    // Where to resume from if the current `*` turns out to have consumed too
    // little: the classic backtracking match, which stays linear in practice
    // because a ref pattern has one or two wildcards.
    let (mut star, mut resume) = (None, 0);

    while v < value.len() {
        if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            resume = v;
            p += 1;
        } else if p < pattern.len() && pattern[p] == value[v] {
            p += 1;
            v += 1;
        } else if let Some(last_star) = star {
            p = last_star + 1;
            resume += 1;
            v = resume;
        } else {
            return false;
        }
    }

    pattern[p..].iter().all(|&c| c == '*')
}

#[derive(Clone, Debug)]
pub struct Stage {
    pub name: String,
    /// Stages that must finish before this one starts.
    pub needs: Vec<String>,
    /// When absent, the stage always runs.
    pub when: Option<Condition>,
    pub jobs: Vec<Job>,
}

#[derive(Clone, Debug)]
pub struct Job {
    pub name: String,
    /// Evaluated on top of the stage's own condition: a job runs only if both
    /// its stage and it say so.
    pub when: Option<Condition>,
    /// Extra environment for every step of this job.
    pub env: BTreeMap<String, String>,
    /// Names of secrets to inject. A secret not named here is not visible to
    /// the job, so a pipeline lists what it actually needs.
    pub secrets: Vec<String>,
    /// Seconds. `None` means the deployment's `CONVEYOR_JOB_TIMEOUT_SECS`.
    pub timeout: Option<u64>,
    /// Container image, honoured by the kubernetes executor and ignored by the
    /// native one, which has only the toolchain conveyor itself was given.
    pub image: Option<String>,
    /// Paths to collect once the job succeeds.
    pub artifacts: Vec<String>,
    pub steps: Vec<Step>,
}

/// One command, and who runs it.
///
/// `Run` is the escape hatch; the others exist so a failure is attributable to
/// a tool rather than to a shell line, and so the arguments can be checked
/// before anything is executed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Run(String),
    Anvil(String),
    Riveter(String),
    Warehouse(String),
}

impl Step {
    /// Every tag a step may carry in `.conveyor.toml`, for parsing and for the
    /// error message when none of them matched.
    pub const KINDS: [&'static str; 4] = ["run", "anvil", "riveter", "warehouse"];

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Run(_) => "run",
            Self::Anvil(_) => "anvil",
            Self::Riveter(_) => "riveter",
            Self::Warehouse(_) => "warehouse",
        }
    }

    pub fn command(&self) -> &str {
        match self {
            Self::Run(command)
            | Self::Anvil(command)
            | Self::Riveter(command)
            | Self::Warehouse(command) => command,
        }
    }

    pub fn new(kind: &str, command: impl Into<String>) -> Option<Self> {
        let command = command.into();
        match kind {
            "run" => Some(Self::Run(command)),
            "anvil" => Some(Self::Anvil(command)),
            "riveter" => Some(Self::Riveter(command)),
            "warehouse" => Some(Self::Warehouse(command)),
            _ => None,
        }
    }
}
