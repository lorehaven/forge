//! One fetch-merge-store cycle across every source, and the timer that repeats it.

use std::time::Duration;

use quench_db::prelude::Db;
use quench_log::{error, info};
use tokio::task::JoinSet;

use crate::config::Settings;
use crate::connectors::{Source, client, izzy_stats};
use crate::merge::{DEFAULT_SOURCE_PRIORITY, add_downloads, merge_all};
use crate::models::RawListing;
use crate::store;

/// What each source answered, by name.
pub type Fetched = Vec<(String, Result<Vec<RawListing>, String>)>;

/// Fetches every source at once. A source that fails is logged and counts as empty, so one dead upstream does not
/// blank the apps the others still offer.
pub async fn fetch_all(sources: Vec<Source>) -> Fetched {
    let http = client();
    let mut set = JoinSet::new();
    for source in sources {
        let http = http.clone();
        set.spawn(async move { (source.name().to_string(), source.fetch(&http).await) });
    }
    let mut out = Vec::new();
    while let Some(done) = set.join_next().await {
        if let Ok(answer) = done {
            out.push(answer);
        }
    }
    out
}

/// Merges what the sources gave and stores it.
///
/// When every source failed nothing is written: a stale catalogue beats an empty one after a network blip. Returns how
/// many packages were written (0 when skipped).
///
/// # Errors
/// The store failed.
pub async fn apply(db: &Db, fetched: Fetched) -> Result<usize, String> {
    apply_with_downloads(db, fetched, &std::collections::HashMap::new()).await
}

/// [`apply`], with download counts for the packages the sources gave none for.
///
/// # Errors
/// The store failed.
pub async fn apply_with_downloads<S: std::hash::BuildHasher + Sync>(
    db: &Db,
    fetched: Fetched,
    downloads: &std::collections::HashMap<String, i64, S>,
) -> Result<usize, String> {
    let mut by_source = std::collections::HashMap::new();
    let mut any = false;
    for (name, result) in fetched {
        match result {
            Err(why) => {
                error!("source {name} failed: {why}");
                by_source.insert(name, Vec::new());
            }
            Ok(listings) => {
                info!("source {name}: {} listings", listings.len());
                by_source.insert(name, listings);
                any = true;
            }
        }
    }
    if !any {
        error!("every source failed; keeping the catalogue as it is");
        return Ok(0);
    }
    let mut merged = merge_all(by_source, DEFAULT_SOURCE_PRIORITY);
    add_downloads(&mut merged, downloads);
    let count = merged.len();
    store::replace_all(db, &merged)
        .await
        .map_err(|e| e.to_string())?;
    info!("refresh complete: {count} packages");
    Ok(count)
}

/// Starts the refresh: one at once, so a fresh deploy does not serve an empty catalogue for an hour, then one every
/// interval.
pub fn spawn_scheduler(db: Db, settings: Settings) {
    tokio::spawn(async move {
        let mut tick =
            tokio::time::interval(Duration::from_secs(settings.refresh_interval_minutes * 60));
        loop {
            // The first tick is immediate.
            tick.tick().await;
            // The counts are an extra: without them (the site is down) the catalogue is just as it was.
            let downloads = izzy_stats::fetch(&client(), &settings.izzy_stats_url)
                .await
                .unwrap_or_else(|why| {
                    error!("{why}");
                    std::collections::HashMap::new()
                });
            let fetched = fetch_all(Source::all(&settings)).await;
            if let Err(why) = apply_with_downloads(&db, fetched, &downloads).await {
                error!("refresh failed: {why}");
            }
        }
    });
}
