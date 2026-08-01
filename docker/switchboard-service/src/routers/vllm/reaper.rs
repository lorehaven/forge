use crate::routers::vllm::engine::VllmEngine;
use std::sync::Arc;
use std::time::Duration;

/// How often the reaper checks for `Failed` instances.
const REAP_INTERVAL_SECS: u64 = 30;

/// vLLM pods never restart (`restartPolicy: Never` in `kubernetes.rs`), so a
/// crash leaves the pod parked in `Failed` phase forever - and because the
/// launcher names pods after the model, a stale `Failed` pod blocks every
/// future relaunch of that model with "already exists" until something
/// deletes it. Nothing upstream does that automatically, so this does.
pub fn spawn_reaper(engine: Arc<dyn VllmEngine>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(REAP_INTERVAL_SECS));
        loop {
            interval.tick().await;
            reap_failed(&engine).await;
        }
    });
}

async fn reap_failed(engine: &Arc<dyn VllmEngine>) {
    let instances = match engine.list_instances().await {
        Ok(instances) => instances,
        Err(err) => {
            tracing::error!("vLLM reaper: failed to list instances: {}", err);
            return;
        }
    };

    for instance in instances.iter().filter(|i| i.status == "Failed") {
        tracing::warn!(
            "vLLM reaper: removing failed instance {} ({})",
            instance.id,
            instance.model
        );
        if let Err(err) = engine.stop_instance(instance.id.clone()).await {
            tracing::error!("vLLM reaper: failed to remove {}: {}", instance.id, err);
        }
    }
}
