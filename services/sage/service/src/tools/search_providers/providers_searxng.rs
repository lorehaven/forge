use super::SearchProvider;
use quench_core::async_trait;
use quench_serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAX_RESULTS: usize = 5;

/// Cached answers are kept this long unless `SEARXNG_CACHE_TTL_SECS` says otherwise. Engines block
/// an address that asks too often, so a repeated query (a retry, a follow-up, the same popular
/// question from two users) is better served from memory than sent again.
const DEFAULT_CACHE_TTL_SECS: u64 = 600;
/// The cache holds at most this many queries; the oldest entry goes when it is full.
const CACHE_CAPACITY: usize = 256;

#[derive(Debug)]
pub struct SearxngProvider {
    client: quench_client::transport::Client,
    instance_url: String,
    cache: Mutex<HashMap<String, (Instant, String)>>,
    ttl: Duration,
}

#[derive(Deserialize, Debug)]
struct SearxngResponse {
    results: Option<Vec<SearxngResult>>,
}

#[derive(Deserialize, Debug)]
struct SearxngResult {
    title: String,
    url: String,
    content: Option<String>,
}

impl SearxngProvider {
    #[must_use]
    pub fn new(instance_url: &str) -> Self {
        Self {
            client: quench_client::transport::Client::new(),
            instance_url: instance_url.trim_end_matches('/').to_string(),
            cache: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(DEFAULT_CACHE_TTL_SECS),
        }
    }

    /// Sets how long an answer is reused; zero turns the cache off.
    #[must_use]
    pub const fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    fn cached(&self, key: &str) -> Option<String> {
        let (at, body) = self.cache.lock().ok()?.get(key)?.clone();
        (at.elapsed() < self.ttl).then_some(body)
    }

    fn remember(&self, key: String, body: &str) {
        if self.ttl.is_zero() {
            return;
        }
        let Ok(mut cache) = self.cache.lock() else {
            return;
        };
        let ttl = self.ttl;
        cache.retain(|_, (at, _)| at.elapsed() < ttl);
        if cache.len() >= CACHE_CAPACITY {
            let oldest = cache
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(k, _)| k.clone());
            if let Some(oldest) = oldest {
                cache.remove(&oldest);
            }
        }
        cache.insert(key, (Instant::now(), body.to_string()));
    }

    pub fn from_env() -> Result<Self, String> {
        let instance_url = std::env::var("SEARXNG_INSTANCE_URL")
            .unwrap_or_else(|_| "https://searxng.be".to_string());
        let ttl = std::env::var("SEARXNG_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_CACHE_TTL_SECS);
        Ok(Self::new(&instance_url).with_cache_ttl(Duration::from_secs(ttl)))
    }
}

#[async_trait]
impl SearchProvider for SearxngProvider {
    fn name(&self) -> &'static str {
        "searxng"
    }

    fn requires_api_key(&self) -> bool {
        false
    }

    async fn search(&self, query: &str) -> Result<String, String> {
        let key = query.trim().to_lowercase();
        if let Some(hit) = self.cached(&key) {
            quench_log::info!("SearXNG: cache hit for '{}'", query);
            return Ok(hit);
        }
        // The language of the query decides the language of the results; left to itself an engine
        // follows the server's location and answers an English question with pages in Polish.
        let url = format!(
            "{}/search?q={}&format=json&pageno=1&language={}",
            self.instance_url,
            quench_serde::urlencoded::percent_encode(query),
            crate::grounding::detect_lang(query)
        );

        quench_log::info!("SearXNG: searching for '{}' at {}", query, url);

        let response = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| {
                let err_msg = format!("Request failed: {e}");
                quench_log::warn!("SearXNG request failed: {}", e);
                err_msg
            })?;

        let status = response.status();
        quench_log::info!("SearXNG response status: {}", status);

        let body = response.text().await.map_err(|e| {
            quench_log::warn!("Failed to read response: {}", e);
            format!("Failed to read response: {e}")
        })?;

        quench_log::info!("SearXNG response length: {} bytes", body.len());

        let data: SearxngResponse = quench_serde::json::from_str(&body).map_err(|e| {
            quench_log::warn!("Failed to parse JSON: {}", e);
            format!("Invalid JSON: {e}")
        })?;

        let mut results = Vec::new();
        if let Some(search_results) = data.results {
            for result in search_results.iter().take(MAX_RESULTS) {
                let snippet = result.content.as_deref().unwrap_or(&result.title).trim();

                let formatted = format!("{}\nSource: {}", snippet, result.url);
                results.push(formatted);
            }
        }

        if results.is_empty() {
            quench_log::warn!("SearXNG: No results found for '{}'", query);

            return Err(format!(
                "Web search for '{query}' could not find results at this time."
            ));
        }

        let result_count = results.len();
        quench_log::info!("SearXNG: Found {} results for '{}'", result_count, query);
        let answer = format!(
            "Search results for '{}' (via SearXNG)\n\n{}",
            query,
            results.join("\n---\n")
        );
        self.remember(key, &answer);
        Ok(answer)
    }
}
