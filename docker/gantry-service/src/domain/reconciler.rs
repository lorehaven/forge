//! The loop that makes operations real. Everything it needs is in the operation row and the runner, so it
//! can be killed at any point and the next tick (in this process or a new one) picks up exactly there:
//!
//! - a `running` row whose runner is still going is left alone - that is *adoption*;
//! - one whose runner finished is recorded, with the runner's log kept;
//! - one whose runner is missing is started again (the service died between marking it and creating it);
//!   steps are idempotent, so running one twice is safe;
//! - then the next queued operation is claimed - the store, not this code, refuses a second one in a scope.

use crate::domain::GantryError;
use crate::domain::cluster::JobStatus;
use crate::domain::executor::Launch;
use crate::domain::operation::{Operation, State, job_name};
use crate::domain::runner::{Outcome, parse_outcome};
use crate::domain::service::Gantry;
use std::sync::Arc;
use std::time::Duration;

/// What one tick did; the tests read it, the service logs it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub adopted: usize,
    pub finished: usize,
    pub restarted: usize,
    pub started: usize,
}

/// The log is kept for the history; the tail is what says why it ended.
const LOG_KEEP: usize = 64 * 1024;

fn tail(log: &str) -> String {
    if log.len() <= LOG_KEEP {
        return log.to_string();
    }
    let mut start = log.len() - LOG_KEEP;
    while !log.is_char_boundary(start) {
        start += 1;
    }
    format!("... (earlier output dropped)\n{}", &log[start..])
}

fn launch(operation: &Operation) -> Launch {
    Launch {
        operation_id: operation.id.clone(),
        name: operation
            .runner_job
            .clone()
            .unwrap_or_else(|| job_name(&operation.id)),
        plan: operation.plan.clone(),
    }
}

pub async fn tick(gantry: &Gantry) -> Result<Report, GantryError> {
    let mut report = Report::default();

    for operation in gantry.store.running().await? {
        settle(gantry, &operation, &mut report).await?;
    }

    // One claim at a time; the store refuses while a scope is busy, so this ends by itself.
    for _ in 0..16 {
        let Some(operation) = gantry.store.claim_next().await? else {
            break;
        };
        match gantry.executor.start(&launch(&operation)).await {
            Ok(()) => report.started += 1,
            Err(error) => {
                gantry
                    .store
                    .finish(
                        &operation.id,
                        State::Failed,
                        Some(&format!("could not start the runner: {error}")),
                        None,
                        None,
                    )
                    .await?;
            }
        }
    }
    Ok(report)
}

async fn settle(
    gantry: &Gantry,
    operation: &Operation,
    report: &mut Report,
) -> Result<(), GantryError> {
    let launch = launch(operation);

    if operation.cancel_requested {
        let log = gantry.executor.log(&launch.name).await.unwrap_or_default();
        let _ = gantry.executor.stop(&launch.name).await;
        gantry
            .store
            .finish(
                &operation.id,
                State::Cancelled,
                Some("cancelled; steps already run were not undone"),
                Some(&tail(&log)),
                None,
            )
            .await?;
        report.finished += 1;
        return Ok(());
    }

    // A cluster that cannot be reached right now says nothing about the operation: leave it running.
    let status = match gantry.executor.status(&launch.name).await {
        Ok(status) => status,
        Err(error) => {
            tracing::warn!(
                "operation {}: runner status unavailable: {error}",
                operation.id
            );
            return Ok(());
        }
    };

    match status {
        JobStatus::Active => report.adopted += 1,
        JobStatus::Absent => {
            tracing::warn!(
                "operation {}: runner missing, starting it again",
                operation.id
            );
            if gantry.executor.start(&launch).await.is_ok() {
                report.restarted += 1;
            }
        }
        JobStatus::Succeeded | JobStatus::Failed(_) => {
            let log = gantry.executor.log(&launch.name).await.unwrap_or_default();
            let reported = parse_outcome(&log);
            let (state, error) = match (&status, reported) {
                (_, Some(Outcome::RolledBack(reason))) => (State::RolledBack, Some(reason)),
                (JobStatus::Succeeded, Some(Outcome::Succeeded) | None) => (State::Succeeded, None),
                (_, Some(Outcome::Failed(reason))) => (State::Failed, Some(reason)),
                (JobStatus::Failed(message), _) => (State::Failed, Some(message.clone())),
                (JobStatus::Active | JobStatus::Absent, _) => unreachable!("handled above"),
            };
            // Recorded before the operation is, and only on success: if this is interrupted the next tick
            // does it again (setting a state twice is the same as once), and a failure records nothing.
            if state == State::Succeeded {
                record_inventory(gantry, operation, &log).await?;
                crate::domain::deployments::apply_effects(
                    gantry,
                    &operation.plan,
                    &operation.requested_by,
                )
                .await?;
            }
            gantry
                .store
                .finish(
                    &operation.id,
                    state,
                    error.as_deref(),
                    Some(&tail(&log)),
                    Some(serde_json::json!({ "runner": launch.name })),
                )
                .await?;
            let _ = gantry.executor.cleanup(&launch.name).await;
            report.finished += 1;
        }
    }
    Ok(())
}

/// After an install, remember what the package declares: that is what lets a resource that is deleted
/// afterwards still be listed, and applied again.
async fn record_inventory(
    gantry: &Gantry,
    operation: &Operation,
    log: &str,
) -> Result<(), GantryError> {
    let installed = operation.plan.steps.iter().find_map(|step| match step {
        // An install, or a refresh (a check with nothing applied): either renders the whole package.
        crate::domain::steps::Step::Install {
            package, version, ..
        }
        | crate::domain::steps::Step::Check {
            package, version, ..
        } => Some((package.clone(), version.clone())),
        _ => None,
    });
    let (Some((package, version)), Some(resources)) =
        (installed, crate::domain::resources::inventory_from_log(log))
    else {
        return Ok(());
    };
    gantry
        .store
        .set_inventory(
            &package,
            &crate::domain::operation::Inventory { version, resources },
        )
        .await
}

/// Ticks forever: when asked (a submit or a cancel), and every few seconds regardless.
pub async fn run(gantry: Arc<Gantry>) {
    loop {
        match tick(&gantry).await {
            Ok(report) => {
                if report != Report::default() {
                    tracing::debug!("reconciler: {report:?}");
                }
            }
            Err(error) => tracing::error!("reconciler: {error}"),
        }
        tokio::select! {
            () = gantry.woken() => {}
            () = tokio::time::sleep(Duration::from_secs(3)) => {}
        }
    }
}
