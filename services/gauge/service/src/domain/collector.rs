//! One task per host: scrape, record, wait, repeat. A failing host keeps its last view and shows the error.

use super::config::{Config, Target};
use super::sources;
use super::store::Store;
use std::time::Duration;

pub fn spawn(config: &Config, store: &Store) {
    for target in config.targets.clone() {
        let store = store.clone();
        let every = Duration::from_secs(config.interval_secs);
        tokio::spawn(async move { watch(target, store, every).await });
    }
}

async fn watch(target: Target, store: Store, every: Duration) {
    let http = sources::client();
    let mut was_up = true;
    loop {
        let result = sources::scrape(&http, &target).await;
        if let (Err(error), true) = (&result, was_up) {
            quench_log::warn!("{}: scrape failed: {error}", target.name);
        }
        was_up = result.is_ok();
        store.record(&target.name, result);
        tokio::time::sleep(every).await;
    }
}
