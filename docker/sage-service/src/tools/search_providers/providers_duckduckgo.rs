use super::SearchProvider;
use async_trait::async_trait;
use scraper::{Html, Selector};

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
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );

        tracing::info!("DuckDuckGo: searching for '{}' at {}", query, url);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
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

        let document = Html::parse_document(&body);
        let mut results = Vec::new();
        const MAX_RESULTS: usize = 5;

        // Parse search results from HTML
        // DuckDuckGo HTML response uses <div class="result"> for each result
        let result_selector = Selector::parse(".result")
            .map_err(|e| format!("Failed to parse HTML selector: {:?}", e))?;

        let title_selector = Selector::parse("a.result__a")
            .map_err(|e| format!("Failed to parse title selector: {:?}", e))?;

        let snippet_selector = Selector::parse("a.result__snippet")
            .map_err(|e| format!("Failed to parse snippet selector: {:?}", e))?;

        for result_elem in document.select(&result_selector).take(MAX_RESULTS) {
            // Get title and URL
            if let Some(title_elem) = result_elem.select(&title_selector).next()
                && let Some(href) = title_elem.value().attr("href")
            {
                // Get snippet
                let snippet_text =
                    if let Some(snippet_elem) = result_elem.select(&snippet_selector).next() {
                        snippet_elem.inner_html()
                    } else {
                        title_elem.inner_html()
                    };

                let clean_snippet = snippet_text
                    .replace("<b>", "")
                    .replace("</b>", "")
                    .trim()
                    .to_string();

                let formatted = format!("{}\nSource: {}", clean_snippet, href);
                results.push(formatted);
            }
        }

        if results.is_empty() {
            tracing::warn!(
                "DuckDuckGo: No results found for '{}' - may need to adjust selectors",
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
