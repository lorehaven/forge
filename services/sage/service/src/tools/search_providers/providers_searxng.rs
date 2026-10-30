use super::SearchProvider;
use quench_core::async_trait;
use quench_serde::Deserialize;

const MAX_RESULTS: usize = 5;

#[derive(Debug)]
pub struct SearxngProvider {
    client: quench_client::transport::Client,
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
    #[must_use]
    pub fn new(instance_url: &str) -> Self {
        Self {
            client: quench_client::transport::Client::new(),
            instance_url: instance_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn from_env() -> Result<Self, String> {
        let instance_url = std::env::var("SEARXNG_INSTANCE_URL")
            .unwrap_or_else(|_| "https://searxng.be".to_string());
        Ok(Self::new(&instance_url))
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
        let url = format!(
            "{}/search?q={}&format=json&pageno=1",
            self.instance_url,
            quench_serde::urlencoded::percent_encode(query)
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
        Ok(format!(
            "Search results for '{}' (via SearXNG)\n\n{}",
            query,
            results.join("\n---\n")
        ))
    }
}
