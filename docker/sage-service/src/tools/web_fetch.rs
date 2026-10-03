use super::{ToolCall, ToolDefinition, ToolExecutor, ToolParameters, ToolResult};
use async_trait::async_trait;
use quench_cache::DataCache;
use serde_json::json;
use std::sync::Arc;

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
    cache: Arc<DataCache>,
}

impl WebFetchExecutor {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            cache: Arc::new(DataCache::new()),
        }
    }

    fn get_cached(&self, url: &str) -> Option<String> {
        self.cache
            .get(url)
            .and_then(|val| val.as_str().map(|s| s.to_string()))
    }

    fn cache_result(&self, url: String, content: String) {
        self.cache.set_with_ttl(url, json!(content), 300); // 5 min TTL
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
                    };
                }
            },
            None => {
                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: "Missing 'url' argument".to_string(),
                    is_error: true,
                };
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

/// Fetch a page and return its extracted text (title, headings, paragraphs) capped at
/// `max_chars`; for the grounding pipeline, which wants far more than the tool's 2000 chars.
pub async fn fetch_page_text(
    client: &reqwest::Client,
    url: &str,
    max_chars: usize,
) -> Result<String, String> {
    let html = fetch_html(client, url).await?;
    extract_rich_text(&html, max_chars)
}

/// Elements whose subtree is page chrome or non-content, never worth quoting.
const SKIPPED_ANCESTORS: &[&str] = &[
    "nav", "footer", "aside", "form", "noscript", "script", "style", "template", "svg",
];

/// JSON-LD keys worth surfacing: they carry the facts of pages that render client-side.
const JSON_LD_KEYS: &[&str] = &[
    "headline",
    "name",
    "description",
    "articleBody",
    "datePublished",
    "dateModified",
    "startDate",
    "endDate",
];

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collect_json_ld(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                match val {
                    serde_json::Value::String(text) if JSON_LD_KEYS.contains(&key.as_str()) => {
                        let text = collapse_ws(text);
                        if !text.is_empty() && out.len() < 12 {
                            out.push(format!(
                                "{key}: {}",
                                text.chars().take(600).collect::<String>()
                            ));
                        }
                    }
                    _ => collect_json_ld(val, out),
                }
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| collect_json_ld(v, out)),
        _ => {}
    }
}

/// Extracts a page's readable content as paragraphs separated by blank lines: title, meta
/// description, JSON-LD facts, then headings, paragraphs, list items, quotes and table rows
/// (cells joined with ` | `) in document order, preferring `<main>`/`<article>` and skipping
/// navigation, footers and forms. Capped at `max_chars`.
pub fn extract_rich_text(html: &str, max_chars: usize) -> Result<String, String> {
    use scraper::{ElementRef, Html, Selector};
    use std::collections::HashSet;

    let document = Html::parse_document(html);
    let sel = |css: &str| Selector::parse(css).map_err(|e| format!("bad selector {css}: {e:?}"));
    let mut blocks: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push = |text: String, blocks: &mut Vec<String>| {
        if !text.is_empty() && seen.insert(text.clone()) {
            blocks.push(text);
        }
    };

    if let Some(title) = document.select(&sel("title")?).next() {
        let text = collapse_ws(&title.text().collect::<String>());
        push(format!("# {text}"), &mut blocks);
    }
    for meta in document.select(&sel(
        r#"meta[name="description"], meta[property="og:description"]"#,
    )?) {
        if let Some(content) = meta.value().attr("content") {
            push(collapse_ws(content), &mut blocks);
        }
    }

    let mut ld = Vec::new();
    for script in document.select(&sel(r#"script[type="application/ld+json"]"#)?) {
        if let Ok(value) =
            serde_json::from_str::<serde_json::Value>(&script.text().collect::<String>())
        {
            collect_json_ld(&value, &mut ld);
        }
    }
    for entry in ld {
        push(entry, &mut blocks);
    }

    let root = ["main", "article", "[role=main]", "body"]
        .iter()
        .find_map(|css| sel(css).ok().and_then(|s| document.select(&s).next()))
        .unwrap_or_else(|| document.root_element());

    let block_sel = sel("h1, h2, h3, h4, p, li, tr, blockquote, pre, figcaption, dt, dd")?;
    let cell_sel = sel("td, th")?;
    for el in root.select(&block_sel) {
        let skipped = el
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|a| SKIPPED_ANCESTORS.contains(&a.value().name()));
        if skipped {
            continue;
        }
        let name = el.value().name();
        let text = if name == "tr" {
            let cells: Vec<String> = el
                .select(&cell_sel)
                .map(|c| collapse_ws(&c.text().collect::<String>()))
                .filter(|c| !c.is_empty())
                .collect();
            cells.join(" | ")
        } else {
            collapse_ws(&el.text().collect::<String>())
        };
        let min = match name {
            "p" => 25,
            "li" | "dd" | "dt" | "blockquote" | "figcaption" => 15,
            "tr" => 6,
            _ => 3,
        };
        if text.chars().count() < min {
            continue;
        }
        let text = match name {
            "h1" => format!("## {text}"),
            "h2" | "h3" | "h4" => format!("### {text}"),
            "li" => format!("- {text}"),
            _ => text,
        };
        push(text, &mut blocks);
    }

    let mut text = blocks.join("\n\n");
    if text.chars().count() < 100 {
        return Err("No extractable text found on page".to_string());
    }
    if text.len() > max_chars {
        let mut cut = max_chars;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n\n[Content truncated...]");
    }
    Ok(text)
}

async fn fetch_and_extract(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let html = fetch_html(client, url).await?;
    extract_text(&html)
}

async fn fetch_html(client: &reqwest::Client, url: &str) -> Result<String, String> {
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
        return Err(format!(
            "HTTP {}: {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown")
        ));
    }

    response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))
}

fn extract_text(html: &str) -> Result<String, String> {
    extract_text_limited(html, 10, 2000)
}

fn extract_text_limited(
    html: &str,
    max_paragraphs: usize,
    max_chars: usize,
) -> Result<String, String> {
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
    if let Ok(selector) = Selector::parse("title")
        && let Some(title) = document.select(&selector).next()
        && let Some(title_text) = title.text().next()
    {
        text.push_str(&format!("# {}\n\n", title_text.trim()));
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
        for p in document.select(&p_selector).take(max_paragraphs) {
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

    // Limit the size to avoid huge responses (cut on a char boundary).
    if text.len() > max_chars {
        let mut cut = max_chars;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n\n[Content truncated...]");
    }

    Ok(text)
}
