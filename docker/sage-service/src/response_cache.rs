use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cached response from a conversation turn
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedResponse {
    pub content: String,
    pub model: String,
    pub timestamp: u64,
    pub tokens_used: Option<u32>,
}

/// Response cache with TTL (24 hours)
pub struct ResponseCache {
    cache: Arc<DashMap<String, CachedResponse>>,
}

impl ResponseCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
        }
    }

    /// Generate cache key from conversation context
    pub fn generate_key(
        conversation_id: &str,
        message: &str,
        model: &str,
        search_provider: &str,
    ) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        format!(
            "{}-{}-{}-{}",
            conversation_id, message, model, search_provider
        )
        .hash(&mut hasher);
        format!("cache_{}", hasher.finish())
    }

    /// Get cached response if valid
    pub fn get(&self, key: &str) -> Option<(CachedResponse, bool)> {
        const CACHE_TTL_SECS: u64 = 24 * 60 * 60; // 24 hours

        if let Some(entry) = self.cache.get(key) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let age = now.saturating_sub(entry.timestamp);
            if age < CACHE_TTL_SECS {
                return Some((entry.clone(), false)); // false = not expired
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

    /// Check if response exists (for "from cache" indicator)
    pub fn has(&self, key: &str) -> bool {
        self.cache.contains_key(key)
    }

    /// Clear cache (optional cleanup)
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Get cache stats
    pub fn stats(&self) -> CacheStats {
        let mut total_size = 0;
        let mut expired_count = 0;

        const CACHE_TTL_SECS: u64 = 24 * 60 * 60;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for entry in self.cache.iter() {
            let age = now.saturating_sub(entry.value().timestamp);
            if age >= CACHE_TTL_SECS {
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

    /// Cleanup expired entries
    pub fn cleanup_expired(&self) -> usize {
        const CACHE_TTL_SECS: u64 = 24 * 60 * 60;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut removed = 0;
        self.cache.retain(|_, entry| {
            let age = now.saturating_sub(entry.timestamp);
            if age >= CACHE_TTL_SECS {
                removed += 1;
                false
            } else {
                true
            }
        });

        removed
    }
}

impl Default for ResponseCache {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_generation() {
        let key1 = ResponseCache::generate_key("conv1", "hello", "model1", "duckduckgo");
        let key2 = ResponseCache::generate_key("conv1", "hello", "model1", "duckduckgo");
        assert_eq!(key1, key2, "Same inputs should produce same key");

        let key3 = ResponseCache::generate_key("conv1", "different", "model1", "duckduckgo");
        assert_ne!(key1, key3, "Different inputs should produce different keys");
    }

    #[test]
    fn test_cache_set_and_get() {
        let cache = ResponseCache::new();
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
        assert_eq!(retrieved.unwrap().0.content, "Test response");
    }

    #[test]
    fn test_cache_stats() {
        let cache = ResponseCache::new();
        cache.set(
            "key1".to_string(),
            CachedResponse {
                content: "Content 1".to_string(),
                model: "model".to_string(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                tokens_used: None,
            },
        );

        let stats = cache.stats();
        assert_eq!(stats.total_entries, 1);
    }
}
