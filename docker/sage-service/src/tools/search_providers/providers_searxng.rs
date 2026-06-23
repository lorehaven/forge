use super::SearchProvider;
use async_trait::async_trait;
use serde::Deserialize;

pub struct SearxngProvider {
    client: reqwest::Client,
    instance_url: String,
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
    pub fn new(instance_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            instance_url: instance_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn from_env() -> Result<Self, String> {
        let instance_url = std::env::var("SEARXNG_INSTANCE_URL")
            .unwrap_or_else(|_| "https://searxng.be".to_string());
        Ok(Self::new(instance_url))
    }
}

#[async_trait]
impl SearchProvider for SearxngProvider {
    fn name(&self) -> &str {
        "searxng"
    }

    fn requires_api_key(&self) -> bool {
        false
    }

    async fn search(&self, query: &str) -> Result<String, String> {
        let url = format!(
            "{}/search?q={}&format=json&pageno=1",
            self.instance_url,
            urlencoding::encode(query)
        );

        tracing::info!("SearXNG: searching for '{}' at {}", query, url);

        let response = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| {
                let err_msg = format!("Request failed: {}", e);
                tracing::warn!("SearXNG request failed: {}", e);
                err_msg
            })?;

        let status = response.status();
        tracing::info!("SearXNG response status: {}", status);

        let body = response.text().await.map_err(|e| {
            tracing::warn!("Failed to read response: {}", e);
            format!("Failed to read response: {}", e)
        })?;

        tracing::info!("SearXNG response length: {} bytes", body.len());

        let data: SearxngResponse = serde_json::from_str(&body).map_err(|e| {
            tracing::warn!("Failed to parse JSON: {}", e);
            format!("Invalid JSON: {}", e)
        })?;

        let mut results = Vec::new();
        const MAX_RESULTS: usize = 5;

        if let Some(search_results) = data.results {
            for result in search_results.iter().take(MAX_RESULTS) {
                let snippet = result.content.as_deref().unwrap_or(&result.title).trim();

                let formatted = format!("{}\nSource: {}", snippet, result.url);
                results.push(formatted);
            }
        }

        if results.is_empty() {
            tracing::warn!("SearXNG: No results found for '{}'", query);

            return Err(format!(
                "Web search for '{}' could not find results at this time.",
                query
            ));
        }

        let result_count = results.len();
        tracing::info!("SearXNG: Found {} results for '{}'", result_count, query);
        Ok(format!(
            "Search results for '{}' (via SearXNG)\n\n{}",
            query,
            results.join("\n---\n")
        ))
    }
}
