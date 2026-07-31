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

    /// Adds `member` to the set held at `key`, creating it when absent.
    ///
    /// Sets are stored as a JSON array. Unlike the plain value calls this one
    /// goes through DashMap's entry API rather than get-then-set: two writers
    /// adding to the same set concurrently must not lose each other's member,
    /// which is the whole reason a caller reaches for a set instead of a value.
    ///
    /// Adding refreshes the set's lifetime, matching Redis `SADD` followed by
    /// `EXPIRE`.
    pub fn add_to_set(&self, key: &str, member: &str, ttl_secs: Option<u64>) {
        let member = Value::String(member.to_string());
        let mut entry = self.cache.entry(key.to_string()).or_insert(CacheEntry {
            value: Value::Array(Vec::new()),
            timestamp: now(),
            ttl_secs,
        });

        // An expired set is a new set: `get` reports it gone, so adding to what
        // is left behind would resurrect members that every reader already
        // considers absent.
        let stale = entry.is_expired() || !entry.value.is_array();
        if stale {
            entry.value = Value::Array(Vec::new());
        }
        if let Value::Array(members) = &mut entry.value
            && !members.contains(&member)
        {
            members.push(member);
        }

        entry.timestamp = now();
        entry.ttl_secs = ttl_secs;
    }

    /// Members of the set at `key`; empty when it is missing, expired, or holds
    /// something that is not a set.
    pub fn set_members(&self, key: &str) -> Vec<String> {
        match self.get(key) {
            Some(Value::Array(members)) => members
                .into_iter()
                .filter_map(|member| member.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Removes `member` from the set at `key`, dropping the key once the set is
    /// empty so it does not outlive its members.
    pub fn remove_from_set(&self, key: &str, member: &str) {
        // The guard is released before `remove`: DashMap shards by key, and
        // holding a reference into the shard while removing from it deadlocks.
        let emptied = {
            let Some(mut entry) = self.cache.get_mut(key) else {
                return;
            };
            match &mut entry.value {
                Value::Array(members) => {
                    members.retain(|held| held.as_str() != Some(member));
                    members.is_empty()
                }
                _ => false,
            }
        };

        if emptied {
            self.cache.remove(key);
        }
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

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub total_size_bytes: usize,
}
