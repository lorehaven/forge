//! Exercises the Redis backend against a real server.
//!
//! Skipped unless `CACHE_TEST_REDIS_URL` is set, so the suite stays runnable
//! without infrastructure. The cluster tests want
//! `CACHE_TEST_REDIS_CLUSTER_URL` - a comma-separated list of seed nodes - and
//! are skipped separately, since a cluster is more than most machines keep
//! running.

#![cfg(feature = "redis")]

use quench_cache::CacheStore;
use quench_cache::store::RedisStore;
use serde_json::json;

fn url() -> Option<String> {
    std::env::var("CACHE_TEST_REDIS_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

fn cluster_url() -> Option<String> {
    std::env::var("CACHE_TEST_REDIS_CLUSTER_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

async fn cluster_store(prefix: &str) -> Option<CacheStore> {
    let url = cluster_url()?;
    Some(CacheStore::Redis(
        RedisStore::connect(&url, prefix)
            .await
            .expect("connect to the cluster"),
    ))
}

#[tokio::test]
async fn values_round_trip_and_expire() {
    let Some(url) = url() else { return };
    let store = CacheStore::Redis(
        RedisStore::connect(&url, "forge-test")
            .await
            .expect("connect"),
    );
    store.clear().await.expect("clear");

    assert!(
        store.is_shared(),
        "a Redis-backed store is visible estate-wide"
    );
    assert_eq!(store.get("missing").await.expect("get"), None);

    store
        .set("greeting", json!({"hello": "world"}), None)
        .await
        .expect("set");
    assert_eq!(
        store.get("greeting").await.expect("get"),
        Some(json!({"hello": "world"}))
    );

    store.remove("greeting").await.expect("remove");
    assert_eq!(store.get("greeting").await.expect("get"), None);

    // One second is the shortest TTL `SET EX` can express.
    store.set("brief", json!(1), Some(1)).await.expect("set");
    assert!(store.get("brief").await.expect("get").is_some());
    tokio::time::sleep(std::time::Duration::from_millis(1400)).await;
    assert_eq!(store.get("brief").await.expect("get"), None, "TTL expired");
}

/// `clear` must take this store's keys and leave everything else alone - the
/// Redis may not be ours exclusively.
#[tokio::test]
async fn clear_is_scoped_to_the_prefix() {
    let Some(url) = url() else { return };
    let ours = CacheStore::Redis(
        RedisStore::connect(&url, "forge-scoped")
            .await
            .expect("connect"),
    );
    let theirs = CacheStore::Redis(
        RedisStore::connect(&url, "somebody-else")
            .await
            .expect("connect"),
    );

    ours.set("a", json!(1), None).await.expect("set");
    theirs.set("a", json!(2), None).await.expect("set");

    ours.clear().await.expect("clear");

    assert_eq!(ours.get("a").await.expect("get"), None);
    assert_eq!(theirs.get("a").await.expect("get"), Some(json!(2)));
    theirs.clear().await.expect("clear");
}

/// The same round trip against a cluster. Keys spread across slots, so this is
/// really a test that the client follows `MOVED` rather than failing on it.
#[tokio::test]
async fn cluster_values_round_trip_and_expire() {
    let Some(store) = cluster_store("forge-cluster").await else {
        return;
    };
    store.clear().await.expect("clear");

    assert!(store.is_shared());
    assert_eq!(store.get("missing").await.expect("get"), None);

    store
        .set("greeting", json!({"hello": "world"}), None)
        .await
        .expect("set");
    assert_eq!(
        store.get("greeting").await.expect("get"),
        Some(json!({"hello": "world"}))
    );

    // `take` is what makes a refresh token single-use, and it has to stay
    // atomic on whichever node owns the slot.
    assert_eq!(
        store.take("greeting").await.expect("take"),
        Some(json!({"hello": "world"}))
    );
    assert_eq!(store.take("greeting").await.expect("take"), None);

    store.set("brief", json!(1), Some(1)).await.expect("set");
    assert!(store.get("brief").await.expect("get").is_some());
    tokio::time::sleep(std::time::Duration::from_millis(1400)).await;
    assert_eq!(store.get("brief").await.expect("get"), None, "TTL expired");
}

/// `clear` has to walk every primary: with enough keys they are certain to
/// land on more than one node, and a single-node `SCAN` would miss most of
/// them while still reporting success.
#[tokio::test]
async fn cluster_clear_sweeps_every_primary() {
    let Some(ours) = cluster_store("forge-cluster-clear").await else {
        return;
    };
    let Some(theirs) = cluster_store("somebody-else-cluster").await else {
        return;
    };
    ours.clear().await.expect("clear");
    theirs.clear().await.expect("clear");

    let keys: Vec<String> = (0..64).map(|n| format!("key-{n}")).collect();
    for key in &keys {
        ours.set(key, json!(1), None).await.expect("set");
    }
    theirs.set("key-0", json!(2), None).await.expect("set");

    ours.clear().await.expect("clear");

    for key in &keys {
        assert_eq!(
            ours.get(key).await.expect("get"),
            None,
            "{key} survived the sweep"
        );
    }
    assert_eq!(
        theirs.get("key-0").await.expect("get"),
        Some(json!(2)),
        "another prefix was swept away"
    );
    theirs.clear().await.expect("clear");
}
