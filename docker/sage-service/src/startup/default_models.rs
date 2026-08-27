use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::Mutex;

use crate::clients::switchboard::{SwitchboardClient, VllmInstance};
use crate::config::{DefaultModel, SageConfig};

/// How many times the monitor retries a default model that never reaches `running` before giving
/// up and moving to the next one; without this, one unlaunchable model would block every model behind it, since launches are serialized.
pub const MAX_LAUNCH_ATTEMPTS: u32 = 3;

/// Instances this process's own monitor has launched, keyed by instance id
/// with the `started_at` switchboard reported at launch time.
///
/// During a rolling update the new pod's monitor typically launches a
/// default model within its first tick, before Kubernetes has even noticed
/// the pod is ready - well before the old pod receives SIGTERM. If shutdown
/// stopped every active instance matching a configured model name, the old
/// pod would tear down the instance the new pod just launched out from under
/// it.
///
/// `owns` decides what shutdown is allowed to stop: an instance this process
/// itself launched and that nothing has relaunched since (compared by
/// `started_at`, not just id - the id is stable across relaunches of the
/// same model/port, so id alone can't tell "still mine" from "replaced"),
/// *or* an instance that already existed before this process even started,
/// which this process merely inherited (e.g. left running by an earlier
/// crash) and is still responsible for cleaning up. What it excludes is
/// exactly the rolling-update case: an instance that appeared after this
/// process started but that a *different* process launched.
#[derive(Clone)]
pub struct LaunchedInstances {
    launches: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
    process_started_at: DateTime<Utc>,
}

impl Default for LaunchedInstances {
    fn default() -> Self {
        Self {
            launches: Arc::new(Mutex::new(HashMap::new())),
            process_started_at: Utc::now(),
        }
    }
}

impl LaunchedInstances {
    async fn record(&self, id: String, started_at: DateTime<Utc>) {
        self.launches.lock().await.insert(id, started_at);
    }

    async fn owns(&self, id: &str, started_at: DateTime<Utc>) -> bool {
        started_at < self.process_started_at
            || self.launches.lock().await.get(id) == Some(&started_at)
    }
}

/// Keep configured default models running: every 10s, ask switchboard what's up and launch the next missing one.
pub fn spawn_monitor(
    switchboard: SwitchboardClient,
    config: SageConfig,
    launched: LaunchedInstances,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        // Consecutive launch attempts per model, kept across ticks so a failing model doesn't stall the rest of the queue.
        let mut attempts: HashMap<String, u32> = HashMap::new();
        loop {
            interval.tick().await;
            monitor_default_models(&switchboard, &config, &mut attempts, &launched).await;
        }
    });
}

/// Instance is still coming up: weights loading, pod scheduling, etc.
fn is_starting(status: &str) -> bool {
    matches!(status, "starting" | "pending")
}

/// Instance counts as covering its model — either up or on its way up.
fn is_active(status: &str) -> bool {
    status == "running" || is_starting(status)
}

/// Pick the next default model to launch. Models are launched **one at a time**: while any
/// instance is still starting this returns `None`, so models don't contend for GPU memory at once.
pub fn next_model_to_launch<'a>(
    config: &'a SageConfig,
    instances: &[VllmInstance],
    attempts: &HashMap<String, u32>,
) -> Option<&'a DefaultModel> {
    if let Some(inst) = instances.iter().find(|inst| is_starting(&inst.status)) {
        tracing::debug!(
            "Instance '{}' is still starting; holding off on launching the next default model.",
            inst.model
        );
        return None;
    }

    config.default_models.iter().find(|model| {
        if !config.is_model_supported(&model.name) {
            return false;
        }
        if attempts
            .get(&model.name)
            .is_some_and(|n| *n >= MAX_LAUNCH_ATTEMPTS)
        {
            return false;
        }
        !instances
            .iter()
            .any(|inst| inst.model == model.name && is_active(&inst.status))
    })
}

pub async fn monitor_default_models(
    switchboard: &SwitchboardClient,
    config: &SageConfig,
    attempts: &mut HashMap<String, u32>,
    launched: &LaunchedInstances,
) {
    tracing::debug!("Checking active instances for default models...");

    let instances = match switchboard.get_vllm_instances().await {
        Ok(inst) => inst,
        Err(err) => {
            tracing::error!(
                "Failed to check running instances for default models: {}",
                err
            );
            return;
        }
    };

    for model in &config.default_models {
        if !config.is_model_supported(&model.name) {
            tracing::warn!(
                "Model '{}' is in default_models but does not match any pattern in supported_models.",
                model.name
            );
            continue;
        }
        // A model that came up successfully starts over with a clean slate, so a later restart is retried rather than treated as exhausted.
        if instances
            .iter()
            .any(|inst| inst.model == model.name && inst.status == "running")
        {
            attempts.remove(&model.name);
        }
    }

    let Some(model) = next_model_to_launch(config, &instances, attempts) else {
        return;
    };

    let attempt = attempts.entry(model.name.clone()).or_insert(0);
    *attempt += 1;
    let attempt = *attempt;

    request_model_launch(switchboard, model, launched).await;

    if attempt >= MAX_LAUNCH_ATTEMPTS {
        tracing::error!(
            "This was attempt {} of {} for default model '{}'. If it does not reach \
             'running' it will be skipped so the remaining default models can start.",
            attempt,
            MAX_LAUNCH_ATTEMPTS,
            model.name
        );
    }
}

pub async fn request_model_launch(
    switchboard: &SwitchboardClient,
    model: &DefaultModel,
    launched: &LaunchedInstances,
) {
    tracing::info!(
        "Default model '{}' is not available. Requesting switchboard to launch it.",
        model.name
    );

    match switchboard
        .launch_instance(
            &model.name,
            model.gpu_memory_utilization,
            model.max_model_len,
            model.quantization.as_deref(),
            model.dtype.as_deref(),
            model.limit_mm_per_prompt.as_deref(),
            model.enable_tool_calling,
            model.task.as_deref(),
            model.device.as_deref(),
        )
        .await
    {
        Ok(inst) => {
            launched.record(inst.id.clone(), inst.started_at).await;
            tracing::info!(
                "Successfully requested launch of model '{}'. Instance ID: {}",
                model.name,
                inst.id
            );
        }
        Err(err) => {
            tracing::error!(
                "Failed to request launch of model '{}': {}",
                model.name,
                err
            );
        }
    }
}

/// Gracefully stop the default models on shutdown: SIGTERM each active instance that matches a
/// configured model *and* that this process itself launched (see `LaunchedInstances`), never one
/// a newer sage replica already relaunched. Best-effort; no-op unless `stop_models_on_shutdown` is enabled.
pub async fn shutdown(
    switchboard: &SwitchboardClient,
    config: &SageConfig,
    launched: &LaunchedInstances,
) {
    if !config.stop_models_on_shutdown {
        tracing::debug!("SAGE_STOP_MODELS_ON_SHUTDOWN is disabled; leaving default models running");
        return;
    }

    tracing::info!("Stopping default models on shutdown...");

    let instances = match switchboard.get_vllm_instances().await {
        Ok(inst) => inst,
        Err(err) => {
            tracing::error!(
                "Failed to list instances while stopping default models: {}",
                err
            );
            return;
        }
    };

    for model in &config.default_models {
        for inst in instances.iter().filter(|inst| {
            inst.model == model.name
                && matches!(inst.status.as_str(), "running" | "starting" | "pending")
        }) {
            if !launched.owns(&inst.id, inst.started_at).await {
                tracing::debug!(
                    "Not stopping '{}' (instance {}): launched by a different sage instance",
                    model.name,
                    inst.id
                );
                continue;
            }

            match switchboard.stop_instance(&inst.id).await {
                Ok(_) => tracing::info!(
                    "Requested graceful stop of default model '{}' (instance {})",
                    model.name,
                    inst.id
                ),
                Err(err) => tracing::error!(
                    "Failed to stop default model '{}' (instance {}): {}",
                    model.name,
                    inst.id,
                    err
                ),
            }
        }
    }
}
