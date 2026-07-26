use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cached HTTP response or request result with TTL
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedResponse {
    pub content: String,
    pub model: String,
    pub timestamp: u64,
    pub tokens_used: Option<u32>,
}

/// Request/response cache with TTL (24 hours default)
pub struct RequestCache {
    cache: Arc<DashMap<String, CachedResponse>>,
    ttl_secs: u64,
}

impl RequestCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            ttl_secs: 24 * 60 * 60, // 24 hours
        }
    }

    pub fn with_ttl(ttl_secs: u64) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            ttl_secs,
        }
    }

    /// Generate cache key from inputs
    pub fn generate_key(parts: &[&str]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        parts.join("-").hash(&mut hasher);
        format!("cache_{}", hasher.finish())
    }

    /// Get cached response if valid and not expired
    pub fn get(&self, key: &str) -> Option<CachedResponse> {
        if let Some(entry) = self.cache.get(key) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let age = now.saturating_sub(entry.timestamp);
            if age < self.ttl_secs {
                return Some(entry.clone());
            }
            // Expired, remove it
            drop(entry);
            self.cache.remove(key);
        }
        None
    }

    /// Store response in cache
    pub fn set(&self, key: String, response: CachedResponse) {
        self.cache.insert(key, response);
    }

    /// Check if key exists in cache
    pub fn has(&self, key: &str) -> bool {
        self.cache.contains_key(key)
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let mut total_size = 0;
        let mut expired_count = 0;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for entry in self.cache.iter() {
            let age = now.saturating_sub(entry.value().timestamp);
            if age >= self.ttl_secs {
                expired_count += 1;
            }
            total_size += entry.value().content.len();
        }

        CacheStats {
            total_entries: self.cache.len(),
            expired_entries: expired_count,
            total_size_bytes: total_size,
        }
    }

    /// Remove expired entries from cache
    pub fn cleanup_expired(&self) -> usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut removed = 0;
        self.cache.retain(|_, entry| {
            let age = now.saturating_sub(entry.timestamp);
            if age >= self.ttl_secs {
                removed += 1;
                false
            } else {
                true
            }
        });

        removed
    }
}

impl Default for RequestCache {
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
