use super::discovery::get_on_disk_model_paths;
use super::store::{get_store, warm_model_cache};
use std::time::Duration;
use tokio::time::sleep;

#[tracing::instrument]
pub async fn sync_models() {
    let store = get_store();

    let on_disk_paths = get_on_disk_model_paths();
    let stored_paths = store.get_all_paths().await;

    // Remove stale models (in DB but not on disk)
    let stale_models: Vec<_> = stored_paths
        .iter()
        .filter(|p| !on_disk_paths.contains(*p))
        .collect();

    if !stale_models.is_empty() {
        tracing::info!("Removing {} stale models from cache.", stale_models.len());
        for path in stale_models {
            store.remove_model(path).await;
        }
    }

    // Discover new models (on disk but not in DB)
    // warm_model_cache internally calls fetch_* which skips paths already in DB
    warm_model_cache().await;
}

pub fn start_sync_job() {
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(60)).await;
            tracing::debug!("Running background model sync...");
            sync_models().await;
        }
    });
}
