//! Unit tests for `data_cache.rs`.

use quench_cache::data_cache::*;

#[test]
fn test_set_and_get() {
    let cache = DataCache::new();
    let key = "user:123".to_string();
    let value = serde_json::json!({"id": 123, "name": "Alice"});

    cache.set(key.clone(), value.clone());
    let retrieved = cache.get(&key);

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), value);
}

#[test]
fn test_ttl_expiration() {
    let cache = DataCache::new();
    let key = "temp_key".to_string();
    let value = serde_json::json!({"data": "temporary"});

    cache.set_with_ttl(key.clone(), value, 0);
    let retrieved = cache.get(&key);

    assert!(retrieved.is_none());
}

#[test]
fn test_delete() {
    let cache = DataCache::new();
    let key = "delete_me".to_string();
    cache.set(key.clone(), serde_json::json!({}));

    assert!(cache.has(&key));
    assert!(cache.delete(&key));
    assert!(!cache.has(&key));
}
