//! Unit tests for `request_cache.rs`.

use quench_cache::request_cache::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_cache_key_generation() {
    let key1 = RequestCache::generate_key(&["conv1", "hello", "model1"]);
    let key2 = RequestCache::generate_key(&["conv1", "hello", "model1"]);
    assert_eq!(key1, key2);

    let key3 = RequestCache::generate_key(&["conv1", "different", "model1"]);
    assert_ne!(key1, key3);
}

#[test]
fn test_cache_set_and_get() {
    let cache = RequestCache::new();
    let key = "test_key".to_string();
    let response = CachedResponse {
        content: "Test response".to_string(),
        model: "test-model".to_string(),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        tokens_used: Some(100),
    };

    cache.set(key.clone(), response.clone());
    let retrieved = cache.get(&key);

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().content, "Test response");
}
