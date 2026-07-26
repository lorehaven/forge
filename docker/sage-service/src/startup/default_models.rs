use std::collections::HashMap;

use crate::clients::switchboard::{SwitchboardClient, VllmInstance};
use crate::config::{DefaultModel, SageConfig};

/// How many times the monitor retries a default model that never reaches
/// `running` before it gives up and moves on to the next one. Without this a
/// single unlaunchable model would block every model behind it forever, since
/// launches are serialized.
pub const MAX_LAUNCH_ATTEMPTS: u32 = 3;

/// Keep the configured default models running: every 10s the monitor asks
/// switchboard which instances are up and launches the next missing one.
pub fn spawn_monitor(switchboard: SwitchboardClient, config: SageConfig) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        // Consecutive launch attempts per model, kept across ticks so a model
        // that keeps failing does not stall the rest of the queue.
        let mut attempts: HashMap<String, u32> = HashMap::new();
        loop {
            interval.tick().await;
            monitor_default_models(&switchboard, &config, &mut attempts).await;
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

/// Pick the next default model to launch. Models are launched **one at a
/// time**: while any instance is still starting this returns `None`, so the
/// next launch waits until the current one has finished loading (or failed)
/// rather than having several models contend for GPU memory at once.
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

async fn monitor_default_models(
    switchboard: &SwitchboardClient,
    config: &SageConfig,
    attempts: &mut HashMap<String, u32>,
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
        // A model that came up successfully starts over with a clean slate, so
        // a later restart is retried rather than treated as exhausted.
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

    request_model_launch(switchboard, model).await;

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

async fn request_model_launch(switchboard: &SwitchboardClient, model: &DefaultModel) {
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
        )
        .await
    {
        Ok(inst) => {
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

/// Gracefully stop the default models on service shutdown. Fetches the active
/// instances and asks switchboard to SIGTERM each one that corresponds to a
/// configured default model. Best-effort: failures are logged, not fatal.
/// Does nothing unless `stop_models_on_shutdown` is enabled.
pub async fn shutdown(switchboard: &SwitchboardClient, config: &SageConfig) {
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
