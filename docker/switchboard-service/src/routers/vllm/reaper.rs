use crate::routers::vllm::engine::VllmEngine;
use std::sync::Arc;
use std::time::Duration;

/// How often the reaper checks for `Failed` instances.
const REAP_INTERVAL_SECS: u64 = 30;

/// A crashed pod (`restartPolicy: Never`) stays parked in `Failed` forever
/// and blocks any relaunch of that model with "already exists"; this reaps it.
pub fn spawn_reaper(engine: Arc<dyn VllmEngine>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(REAP_INTERVAL_SECS));
        loop {
            interval.tick().await;
            reap_failed(&engine).await;
        }
    });
}

pub async fn reap_failed(engine: &Arc<dyn VllmEngine>) {
    let instances = match engine.list_instances().await {
        Ok(instances) => instances,
        Err(err) => {
            quench_log::error!("vLLM reaper: failed to list instances: {}", err);
            return;
        }
    };

    for instance in instances.iter().filter(|i| i.status == "Failed") {
        quench_log::warn!(
            "vLLM reaper: removing failed instance {} ({})",
            instance.id,
            instance.model
        );
        if let Err(err) = engine.stop_instance(instance.id.clone()).await {
            quench_log::error!("vLLM reaper: failed to remove {}: {}", instance.id, err);
        }
    }
}

/// How often the restorer looks for a stopped instance to bring back.
const RESTORE_INTERVAL_SECS: u64 = 30;

/// After a machine reboot the vLLM pods are left `Completed`/interrupted and nothing recreates
/// them; this restarts them, one per tick and only once the previous one is up.
pub fn spawn_restorer(engine: Arc<dyn VllmEngine>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(RESTORE_INTERVAL_SECS));
        loop {
            interval.tick().await;
            match engine.restore_next().await {
                Ok(Some(id)) => quench_log::info!("vLLM restorer: restarted {}", id),
                Ok(None) => {}
                Err(err) => quench_log::error!("vLLM restorer: {}", err),
            }
        }
    });
}
