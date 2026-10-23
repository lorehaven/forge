use super::SearchProvider;
use quench_core::async_trait;

const MAX_RESULTS: usize = 5;

#[derive(Debug)]
pub struct SerpapiProvider {
    client: quench_client::transport::Client,
    api_key: String,
}

impl SerpapiProvider {
    #[must_use]
    pub fn new(api_key: String) -> Self {
        Self {
            client: quench_client::transport::Client::new(),
            api_key,
        }
    }

    pub fn from_env() -> Result<Self, String> {
        let api_key = std::env::var("SERPAPI_API_KEY")
            .map_err(|_| "SERPAPI_API_KEY environment variable not set".to_string())?;
        Ok(Self::new(api_key))
    }
}

#[derive(quench_serde::Deserialize, Debug)]
struct SerpapiResponse {
    #[serde(rename = "answerBox")]
    answer_box: Option<quench_serde::Value>,
    #[serde(rename = "organic_results")]
    organic_results: Option<Vec<SerpapiResult>>,
    #[serde(rename = "knowledgeGraph")]
    knowledge_graph: Option<quench_serde::Value>,
}

#[derive(quench_serde::Deserialize, Debug)]
struct SerpapiResult {
    title: String,
    link: String,
    snippet: Option<String>,
}

#[async_trait]
impl SearchProvider for SerpapiProvider {
    fn name(&self) -> &'static str {
        "serpapi"
    }

    fn requires_api_key(&self) -> bool {
        true
    }

    async fn search(&self, query: &str) -> Result<String, String> {
        let url = format!(
            "https://serpapi.com/search?q={}&api_key={}&engine=google",
            quench_serde::urlencoded::percent_encode(query),
            self.api_key
        );

        quench_log::info!("SerpAPI: searching for '{}'", query);

        let response = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| {
                let err_msg = format!("Request failed: {e}");
                quench_log::warn!("SerpAPI request failed: {}", e);
                err_msg
            })?;

        let status = response.status();
        quench_log::info!("SerpAPI response status: {}", status);

        if status == 401 {
            return Err("SerpAPI: Invalid API key".to_string());
        }

        let body = response.text().await.map_err(|e| {
            quench_log::warn!("Failed to read response: {}", e);
            format!("Failed to read response: {e}")
        })?;

        quench_log::info!("SerpAPI response length: {} bytes", body.len());

        let data: SerpapiResponse = quench_serde::json::from_str(&body).map_err(|e| {
            quench_log::warn!("Failed to parse JSON: {}", e);
            format!("Invalid JSON: {e}")
        })?;

        let mut results = Vec::new();
        // Add knowledge graph if available
        if let Some(kg) = &data.knowledge_graph
            && let Some(description) = kg.get("description").and_then(|v| v.as_str())
        {
            results.push(format!(
                "{}\nSource: {}",
                description,
                kg.get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Knowledge Graph")
            ));
        }

        // Add answer box if available
        if let Some(ab) = &data.answer_box
            && let Some(answer) = ab.get("answer").and_then(|v| v.as_str())
        {
            results.push(answer.to_string());
        }

        // Add organic search results
        if let Some(org_results) = &data.organic_results {
            for result in org_results.iter().take(MAX_RESULTS - results.len()) {
                let snippet = result.snippet.as_deref().unwrap_or(&result.title);
                let formatted = format!("{}\nSource: {}", snippet, result.link);
                results.push(formatted);
            }
        }

        if results.is_empty() {
            quench_log::warn!("SerpAPI: No results found for '{}'", query);
            return Err(format!("Web search for '{query}' found no results."));
        }

        let result_count = results.len();
        quench_log::info!("SerpAPI: Found {} results for '{}'", result_count, query);
        Ok(format!(
            "Search results for '{}' (via SerpAPI)\n\n{}",
            query,
            results.join("\n---\n")
        ))
    }
}
