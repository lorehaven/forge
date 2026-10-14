//! `--env a,b,c`: one command across several environments, one after another, with one report.
//!
//! Environments run in the order given, so a rollout can go canary-first. The first one that fails
//! stops the run: the ones after it are reported as not attempted rather than pushed on top of a
//! failure. Everything that can be checked without touching a cluster is checked for every
//! environment before the first one is applied, so a typo in the third overlay does not surface
//! after the first two have already gone out.

use crate::render::{ResourceScope, Selector, generate_manifests_selected};
use crate::repl::{WaitPolicy, describe, kubectl_apply, kubectl_diff, ok, warn};
use anyhow::Result;
use quench_cli::prelude::{Tone, print_status};

/// What happened to one environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Resources were applied (or, with `--dry-run`, would have been).
    Applied { count: usize, dry_run: bool },
    /// The selection matched nothing, so nothing was sent.
    Nothing,
    /// `diff`: the cluster differs from the manifests.
    Differs,
    /// `diff`: the cluster matches.
    Matches,
    /// The command failed here; the run stopped.
    Failed(String),
    /// An earlier environment failed, so this one was never started.
    NotAttempted,
}

impl Outcome {
    #[must_use]
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::Failed(_) | Self::NotAttempted)
    }

    fn label(&self) -> String {
        match self {
            Self::Applied {
                count,
                dry_run: false,
            } => format!("applied {count} resource(s)"),
            Self::Applied {
                count,
                dry_run: true,
            } => format!("would apply {count} resource(s)"),
            Self::Nothing => "no resources matched".to_string(),
            Self::Differs => "differs from the manifests".to_string(),
            Self::Matches => "matches the manifests".to_string(),
            Self::Failed(why) => format!("FAILED: {}", why.lines().next().unwrap_or("")),
            Self::NotAttempted => "not attempted".to_string(),
        }
    }

    const fn tone(&self) -> Tone {
        match self {
            Self::Failed(_) => Tone::Error,
            Self::NotAttempted | Self::Differs => Tone::Warn,
            _ => Tone::Success,
        }
    }
}

/// One row of the shared report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvReport {
    pub env: String,
    /// The kubectl context the environment is bound to.
    pub context: String,
    pub outcome: Outcome,
}

/// Renders every environment and refuses the run if any of them cannot go ahead, returning each
/// environment's pinned kubectl context.
///
/// An environment that pins no `kube_context` would act on whichever cluster kubectl happens to
/// have selected - for a single environment that is a warning, but across several it would send
/// every one of them to the same place while reporting them as different clusters.
pub fn preflight(
    envs: &[String],
    scope: ResourceScope,
    selector: &Selector,
) -> Result<Vec<String>> {
    let mut contexts = Vec::new();
    let mut problems = Vec::new();

    for env in envs {
        let context = match generate_manifests_selected(env, scope, selector) {
            Ok(rendered) if rendered.kube_context.is_some() => rendered.kube_context,
            Ok(_) => {
                problems.push(format!(
                    "{env}: pins no kube_context - add `kube_context: <name>` to \
                     overlays/{env}/overlay.yaml so each environment is bound to its cluster"
                ));
                None
            }
            Err(err) => {
                problems.push(format!("{env}: {err:#}"));
                None
            }
        };
        contexts.push(context.unwrap_or_default());
    }

    anyhow::ensure!(
        problems.is_empty(),
        "nothing was applied; fix these first:\n  {}",
        problems.join("\n  ")
    );
    Ok(contexts)
}

/// Runs `step` on each environment in turn and stops at the first failure.
///
/// Environments after a failure are present in the report as [`Outcome::NotAttempted`], so the
/// report always has one row per environment asked for.
pub fn run_in_order(
    envs: &[String],
    contexts: &[String],
    mut step: impl FnMut(&str) -> Result<Outcome>,
) -> Vec<EnvReport> {
    let mut reports = Vec::with_capacity(envs.len());
    let mut stopped = false;

    for (env, context) in envs.iter().zip(contexts) {
        let outcome = if stopped {
            Outcome::NotAttempted
        } else {
            match step(env) {
                Ok(outcome) => outcome,
                Err(err) => {
                    stopped = true;
                    Outcome::Failed(format!("{err:#}"))
                }
            }
        };
        reports.push(EnvReport {
            env: env.clone(),
            context: context.clone(),
            outcome,
        });
    }

    reports
}

/// Prints the report, one line per environment, and fails if any environment did.
pub fn finish(reports: &[EnvReport]) -> Result<()> {
    println!();
    let env_width = reports.iter().map(|r| r.env.len()).max().unwrap_or(0);
    let context_width = reports.iter().map(|r| r.context.len()).max().unwrap_or(0);
    for report in reports {
        print_status(
            report.outcome.tone(),
            "report",
            &format!(
                "{:<env_width$}  {:<context_width$}  {}",
                report.env,
                report.context,
                report.outcome.label()
            ),
        );
    }

    let failed = reports
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Failed(_)))
        .count();
    let skipped = reports
        .iter()
        .filter(|r| r.outcome == Outcome::NotAttempted)
        .count();
    if failed == 0 {
        return Ok(());
    }

    // The failing environment's own output has already been printed as it happened; the full
    // message goes last so it is what the terminal ends on.
    let detail = reports
        .iter()
        .find_map(|r| match &r.outcome {
            Outcome::Failed(why) => Some(format!("{}: {why}", r.env)),
            _ => None,
        })
        .unwrap_or_default();
    anyhow::bail!("stopped after a failure ({skipped} environment(s) not attempted)\n\n{detail}")
}

/// `apply` across `envs`.
pub fn apply_many(
    envs: &[String],
    dry_run: bool,
    scope: ResourceScope,
    selector: &Selector,
    wait: WaitPolicy,
) -> Result<()> {
    let contexts = preflight(envs, scope, selector)?;

    let reports = run_in_order(envs, &contexts, |env| {
        ok(&format!("== {env} =="));
        let rendered = kubectl_apply(env, dry_run, scope, selector, wait)?;
        if rendered.resource_count == 0 {
            return Ok(Outcome::Nothing);
        }
        ok(&format!(
            "{} {} resource(s): {}",
            if dry_run { "would apply" } else { "applied" },
            rendered.resource_count,
            describe(&rendered)
        ));
        Ok(Outcome::Applied {
            count: rendered.resource_count,
            dry_run,
        })
    });

    finish(&reports)
}

/// `diff` across `envs`.
pub fn diff_many(envs: &[String], scope: ResourceScope, selector: &Selector) -> Result<()> {
    let contexts = preflight(envs, scope, selector)?;

    let reports = run_in_order(envs, &contexts, |env| {
        ok(&format!("== {env} =="));
        let (rendered, differs) = kubectl_diff(env, scope, selector)?;
        Ok(if rendered.resource_count == 0 {
            Outcome::Nothing
        } else if differs {
            Outcome::Differs
        } else {
            Outcome::Matches
        })
    });

    if reports.iter().any(|r| r.outcome == Outcome::Differs) {
        warn("at least one environment differs from its manifests");
    }
    finish(&reports)
}
