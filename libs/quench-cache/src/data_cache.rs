use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Generic cached data value with optional TTL
#[derive(Clone, Debug)]
pub struct CacheEntry {
    pub value: Value,
    pub timestamp: u64,
    pub ttl_secs: Option<u64>,
}

impl CacheEntry {
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl_secs {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let age = now.saturating_sub(self.timestamp);
            age >= ttl
        } else {
            false
        }
    }
}

/// General-purpose data cache supporting multiple backends
pub struct DataCache {
    cache: Arc<DashMap<String, CacheEntry>>,
}

impl DataCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
        }
    }

    /// Set value with no expiration
    pub fn set(&self, key: String, value: Value) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.cache.insert(
            key,
            CacheEntry {
                value,
                timestamp,
                ttl_secs: None,
            },
        );
    }

    /// Set value with TTL (in seconds)
    pub fn set_with_ttl(&self, key: String, value: Value, ttl_secs: u64) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.cache.insert(
            key,
            CacheEntry {
                value,
                timestamp,
                ttl_secs: Some(ttl_secs),
            },
        );
    }

    /// Get cached value if it exists and hasn't expired
    pub fn get(&self, key: &str) -> Option<Value> {
        if let Some(entry) = self.cache.get(key) {
            if !entry.is_expired() {
                return Some(entry.value.clone());
            }
            drop(entry);
            self.cache.remove(key);
        }
        None
    }

    /// Check if key exists and is valid
    pub fn has(&self, key: &str) -> bool {
        if let Some(entry) = self.cache.get(key) {
            !entry.is_expired()
        } else {
            false
        }
    }

    /// Delete a specific key
    pub fn delete(&self, key: &str) -> bool {
        self.cache.remove(key).is_some()
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let mut total_size = 0;
        let mut expired_count = 0;

        for entry in self.cache.iter() {
            if entry.is_expired() {
                expired_count += 1;
            }
            total_size += entry.value.to_string().len();
        }

        CacheStats {
            total_entries: self.cache.len(),
            expired_entries: expired_count,
            total_size_bytes: total_size,
        }
    }

    /// Remove all expired entries
    pub fn cleanup_expired(&self) -> usize {
        let mut removed = 0;
        self.cache.retain(|_, entry| {
            if entry.is_expired() {
                removed += 1;
                false
            } else {
                true
            }
        });
        removed
    }

    /// Get entry count
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for DataCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub total_size_bytes: usize,
}
