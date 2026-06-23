use super::search_provider::SearchProvider;
use async_trait::async_trait;

pub struct DuckDuckGoProvider {
    client: reqwest::Client,
}

impl DuckDuckGoProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for DuckDuckGoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize, Debug)]
struct DuckDuckGoResponse {
    #[serde(rename = "Abstract")]
    abstract_text: Option<String>,
    #[serde(rename = "AbstractURL")]
    abstract_url: Option<String>,
    #[serde(rename = "RelatedTopics")]
    related_topics: Option<Vec<serde_json::Value>>,
    #[serde(rename = "Results")]
    results: Option<Vec<DuckDuckGoResult>>,
}

#[derive(serde::Deserialize, Debug)]
struct DuckDuckGoResult {
    #[serde(rename = "FirstURL")]
    first_url: String,
    #[serde(rename = "Text")]
    text: String,
}

#[async_trait]
impl SearchProvider for DuckDuckGoProvider {
    fn name(&self) -> &str {
        "duckduckgo"
    }

    fn requires_api_key(&self) -> bool {
        false
    }

    async fn search(&self, query: &str) -> Result<String, String> {
        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_redirect=1&no_html=1&t=sage",
            urlencoding::encode(query)
        );

        tracing::info!("DuckDuckGo: searching for '{}' at {}", query, url);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .header("Accept", "application/json")
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| {
                let err_msg = format!("Request failed: {}", e);
                tracing::warn!("DuckDuckGo request failed: {}", e);
                err_msg
            })?;

        let status = response.status();
        tracing::info!("DuckDuckGo response status: {}", status);

        let body = response.text().await.map_err(|e| {
            tracing::warn!("Failed to read response: {}", e);
            format!("Failed to read response: {}", e)
        })?;

        tracing::info!("DuckDuckGo response length: {} bytes", body.len());
        tracing::warn!("DuckDuckGo raw response (FULL): {}", body);

        let data: DuckDuckGoResponse = serde_json::from_str(&body).map_err(|e| {
            tracing::warn!("Failed to parse JSON: {}", e);
            format!("Invalid JSON: {}", e)
        })?;

        let mut results = Vec::new();
        const MAX_RESULTS: usize = 5;

        // Add abstract answer if available (primary result)
        if let Some(abstract_text) = &data.abstract_text
            && !abstract_text.is_empty()
        {
            let mut result = abstract_text.clone();
            if let Some(url) = &data.abstract_url {
                result.push_str(&format!("\nSource: {}", url));
            }
            results.push(result);
        }

        // Extract from RelatedTopics (this is where DuckDuckGo puts actual results)
        if let Some(related) = &data.related_topics {
            for topic in related {
                if results.len() >= MAX_RESULTS {
                    break;
                }

                if let Some(obj) = topic.as_object() {
                    // Handle grouped topics (they have a "Topics" subarray like "Snakes", "Media", etc)
                    if let Some(subtopics) = obj.get("Topics").and_then(|v| v.as_array()) {
                        for subtopic in subtopics {
                            if results.len() >= MAX_RESULTS {
                                break;
                            }
                            if let Some(sub_obj) = subtopic.as_object()
                                && let Some(text) = sub_obj.get("Text").and_then(|v| v.as_str())
                                && !text.is_empty()
                            {
                                results.push(text.to_string());
                            }
                        }
                    } else if let Some(text) = obj.get("Text").and_then(|v| v.as_str()) {
                        // Handle regular non-grouped topics
                        if !text.is_empty() {
                            results.push(text.to_string());
                        }
                    }
                }
            }
        }

        // Add search results (from direct Results field if any)
        if let Some(search_results) = &data.results {
            for result in search_results.iter().take(MAX_RESULTS - results.len()) {
                let snippet = format!("{}\nSource: {}", result.text, result.first_url);
                results.push(snippet);
            }
        }

        if results.is_empty() {
            tracing::warn!(
                "DuckDuckGo: No results found for '{}' - API may be unavailable or rate-limited",
                query
            );

            return Err(format!(
                "Web search for '{}' could not find results at this time.",
                query
            ));
        }

        let result_count = results.len();
        tracing::info!("DuckDuckGo: Found {} results for '{}'", result_count, query);
        Ok(format!(
            "Search results for '{}' (via DuckDuckGo)\n\n{}",
            query,
            results.join("\n---\n")
        ))
    }
}
