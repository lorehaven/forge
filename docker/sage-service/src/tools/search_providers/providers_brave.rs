use super::SearchProvider;
use async_trait::async_trait;
use serde::Deserialize;

pub struct BraveProvider {
    client: reqwest::Client,
    api_key: String,
}

#[derive(Deserialize, Debug)]
struct BraveResponse {
    web: Option<BraveWebResults>,
}

#[derive(Deserialize, Debug)]
struct BraveWebResults {
    results: Option<Vec<BraveResult>>,
}

#[derive(Deserialize, Debug)]
struct BraveResult {
    title: String,
    url: String,
    description: Option<String>,
}

impl BraveProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
        }
    }

    pub fn from_env() -> Result<Self, String> {
        let api_key = std::env::var("BRAVE_SEARCH_API_KEY")
            .map_err(|_| "BRAVE_SEARCH_API_KEY environment variable not set".to_string())?;
        Ok(Self::new(api_key))
    }
}

#[async_trait]
impl SearchProvider for BraveProvider {
    fn name(&self) -> &str {
        "brave"
    }

    fn requires_api_key(&self) -> bool {
        true
    }

    async fn search(&self, query: &str) -> Result<String, String> {
        let url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={}",
            urlencoding::encode(query)
        );

        tracing::info!("Brave Search: searching for '{}' at {}", query, url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Token {}", self.api_key))
            .header("Accept", "application/json")
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| {
                let err_msg = format!("Request failed: {}", e);
                tracing::warn!("Brave Search request failed: {}", e);
                err_msg
            })?;

        let status = response.status();
        tracing::info!("Brave Search response status: {}", status);

        if status == 401 {
            return Err("Brave Search: Invalid API key".to_string());
        }

        let body = response.text().await.map_err(|e| {
            tracing::warn!("Failed to read response: {}", e);
            format!("Failed to read response: {}", e)
        })?;

        tracing::info!("Brave Search response length: {} bytes", body.len());

        let data: BraveResponse = serde_json::from_str(&body).map_err(|e| {
            tracing::warn!("Failed to parse JSON: {}", e);
            format!("Invalid JSON: {}", e)
        })?;

        let mut results = Vec::new();
        const MAX_RESULTS: usize = 5;

        if let Some(web) = data.web {
            if let Some(search_results) = web.results {
                for result in search_results.iter().take(MAX_RESULTS) {
                    let snippet = result
                        .description
                        .as_deref()
                        .unwrap_or(&result.title)
                        .trim();

                    let formatted = format!(
                        "{}\nSource: {}",
                        snippet,
                        result.url
                    );
                    results.push(formatted);
                }
            }
        }

        if results.is_empty() {
            tracing::warn!(
                "Brave Search: No results found for '{}'",
                query
            );

            return Err(format!(
                "Web search for '{}' could not find results at this time.",
                query
            ));
        }

        let result_count = results.len();
        tracing::info!("Brave Search: Found {} results for '{}'", result_count, query);
        Ok(format!(
            "Search results for '{}' (via Brave Search)\n\n{}",
            query,
            results.join("\n---\n")
        ))
    }
}
