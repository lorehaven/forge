use super::{ToolCall, ToolDefinition, ToolExecutor, ToolParameters, ToolResult};
use async_trait::async_trait;
use serde_json::json;

pub fn get_definition() -> ToolDefinition {
    ToolDefinition {
        name: "web_fetch".to_string(),
        description: "Fetch and extract content from a web page".to_string(),
        tool_type: "function".to_string(),
        parameters: ToolParameters {
            param_type: "object".to_string(),
            properties: json!({
                "url": {
                    "type": "string",
                    "description": "URL of the web page to fetch (must start with http:// or https://)"
                }
            }),
            required: vec!["url".to_string()],
        },
    }
}

pub struct WebFetchExecutor {
    client: reqwest::Client,
    cache: dashmap::DashMap<String, (String, std::time::SystemTime)>,
}

impl WebFetchExecutor {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            cache: dashmap::DashMap::new(),
        }
    }

    fn get_cached(&self, url: &str) -> Option<String> {
        const CACHE_DURATION: std::time::Duration = std::time::Duration::from_secs(300); // 5 min

        if let Some(entry) = self.cache.get(url) {
            let (content, cached_at) = entry.value();
            if let Ok(elapsed) = cached_at.elapsed() {
                if elapsed < CACHE_DURATION {
                    return Some(content.clone());
                }
            }
            drop(entry);
            // Remove expired cache
            self.cache.remove(url);
        }
        None
    }

    fn cache_result(&self, url: String, content: String) {
        self.cache.insert(url, (content, std::time::SystemTime::now()));
    }
}

impl Default for WebFetchExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for WebFetchExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        let url = match tool_call.arguments.get("url") {
            Some(val) => match val.as_str() {
                Some(s) => s.to_string(),
                None => {
                    return ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: "Invalid URL: must be a string".to_string(),
                        is_error: true,
                    }
                }
            },
            None => {
                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: "Missing 'url' argument".to_string(),
                    is_error: true,
                }
            }
        };

        // Validate URL
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: "Invalid URL: must start with http:// or https://".to_string(),
                is_error: true,
            };
        }

        // Check cache first
        if let Some(cached_content) = self.get_cached(&url) {
            return ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: format!("Content from {} (cached)\n\n{}", url, cached_content),
                is_error: false,
            };
        }

        match fetch_and_extract(&self.client, &url).await {
            Ok(content) => {
                self.cache_result(url.clone(), content.clone());
                ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: format!("Content from {}\n\n{}", url, content),
                    is_error: false,
                }
            }
            Err(err) => ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: format!("Failed to fetch webpage: {}", err),
                is_error: true,
            },
        }
    }
}

async fn fetch_and_extract(
    client: &reqwest::Client,
    url: &str,
) -> Result<String, String> {
    let response = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status.as_u16(), status.canonical_reason().unwrap_or("Unknown")));
    }

    let html = response.text().await.map_err(|e| format!("Failed to read response: {}", e))?;

    extract_text(&html)
}

fn extract_text(html: &str) -> Result<String, String> {
    use scraper::{Html, Selector};

    // Remove script and style elements
    let mut clean_html = html.to_string();
    let script_regex = regex::Regex::new(r"(?si)<script[^>]*>.*?</script>").unwrap();
    clean_html = script_regex.replace_all(&clean_html, "").to_string();

    let style_regex = regex::Regex::new(r"(?si)<style[^>]*>.*?</style>").unwrap();
    clean_html = style_regex.replace_all(&clean_html, "").to_string();

    // Parse cleaned HTML
    let document = Html::parse_document(&clean_html);

    // Extract title
    let mut text = String::new();
    if let Ok(selector) = Selector::parse("title") {
        if let Some(title) = document.select(&selector).next() {
            if let Some(title_text) = title.text().next() {
                text.push_str(&format!("# {}\n\n", title_text.trim()));
            }
        }
    }

    // Extract headings and paragraphs
    if let Ok(h1_selector) = Selector::parse("h1") {
        for h1 in document.select(&h1_selector).take(3) {
            let h1_text: String = h1.text().collect::<Vec<_>>().join(" ");
            if !h1_text.trim().is_empty() {
                text.push_str(&format!("## {}\n\n", h1_text.trim()));
            }
        }
    }

    if let Ok(p_selector) = Selector::parse("p") {
        for p in document.select(&p_selector).take(10) {
            let p_text: String = p.text().collect::<Vec<_>>().join(" ");
            let cleaned = p_text.trim();
            if !cleaned.is_empty() && cleaned.len() > 20 {
                text.push_str(&format!("{}\n\n", cleaned));
            }
        }
    }

    if text.trim().is_empty() {
        return Err("No extractable text found on page".to_string());
    }

    // Limit to 2000 characters to avoid huge responses
    if text.len() > 2000 {
        text.truncate(2000);
        text.push_str("\n\n[Content truncated...]");
    }

    Ok(text)
}
