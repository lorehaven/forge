//! Wikipedia as a search provider: the intro of the best-matching articles, straight from the API.
//!
//! Metasearch engines get throttled and blocked, and for an encyclopedic question the answer is
//! in the first paragraph of one article. This provider is cheap, free and stable, so grounding
//! uses it next to the web search (see `grounding::retrieve`), not instead of it.

use super::SearchProvider;
use quench_core::async_trait;
use quench_serde::Deserialize;

const MAX_RESULTS: usize = 3;
const EXTRACT_CHARS: usize = 1100;

#[derive(Debug)]
pub struct WikipediaProvider {
    client: quench_client::transport::Client,
    /// `https://<lang>.wikipedia.org`; a base without `<lang>` (a test server) is used as is.
    base: String,
}

#[derive(Deserialize, Debug)]
struct ApiResponse {
    query: Option<ApiQuery>,
}

#[derive(Deserialize, Debug)]
struct ApiQuery {
    #[serde(default)]
    pages: std::collections::HashMap<String, Page>,
}

#[derive(Deserialize, Debug)]
struct Page {
    title: String,
    #[serde(default)]
    extract: Option<String>,
    #[serde(default)]
    fullurl: Option<String>,
    #[serde(default)]
    index: Option<u32>,
}

impl WikipediaProvider {
    #[must_use]
    pub fn new(base: &str) -> Self {
        Self {
            client: quench_client::transport::Client::new(),
            base: base.trim_end_matches('/').to_string(),
        }
    }

    #[must_use]
    pub fn from_env() -> Self {
        Self::new(&quench_core::env::get_or(
            "WIKIPEDIA_API_BASE",
            "https://<lang>.wikipedia.org",
        ))
    }

    /// The Wikipedia edition that fits the query's language.
    fn edition(&self, query: &str) -> String {
        let lang = crate::grounding::detect_lang(query);
        self.base.replace("<lang>", lang)
    }
}

/// The extract on one line, cut at a sentence end near `max_chars`.
fn tidy(extract: &str, max_chars: usize) -> String {
    let flat = extract.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        return flat;
    }
    let cut: String = flat.chars().take(max_chars).collect();
    match cut.rfind(". ") {
        Some(i) if i > max_chars / 2 => cut[..=i].to_string(),
        _ => cut,
    }
}

#[async_trait]
impl SearchProvider for WikipediaProvider {
    fn name(&self) -> &'static str {
        "wikipedia"
    }

    fn requires_api_key(&self) -> bool {
        false
    }

    async fn search(&self, query: &str) -> Result<String, String> {
        let url = format!(
            "{}/w/api.php?action=query&format=json&generator=search&gsrsearch={}&gsrlimit={MAX_RESULTS}\
             &prop=extracts%7Cinfo&exintro=1&explaintext=1&exlimit={MAX_RESULTS}&inprop=url",
            self.edition(query),
            quench_serde::urlencoded::percent_encode(query)
        );
        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Sage/0.6 (grounded web assistant)")
            .timeout(std::time::Duration::from_secs(6))
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("Wikipedia answered {}", response.status().as_u16()));
        }
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {e}"))?;
        let data: ApiResponse =
            quench_serde::json::from_str(&body).map_err(|e| format!("Invalid JSON: {e}"))?;

        let mut pages: Vec<Page> = data
            .query
            .map(|q| q.pages.into_values().collect())
            .unwrap_or_default();
        pages.sort_by_key(|p| p.index.unwrap_or(u32::MAX));

        let results: Vec<String> = pages
            .iter()
            .filter_map(|p| {
                let extract = tidy(p.extract.as_deref()?, EXTRACT_CHARS);
                let url = p.fullurl.as_deref()?;
                (!extract.is_empty()).then(|| format!("{}: {extract}\nSource: {url}", p.title))
            })
            .collect();
        if results.is_empty() {
            return Err(format!("Wikipedia has nothing for '{query}'."));
        }
        Ok(format!(
            "Search results for '{query}' (via Wikipedia)\n\n{}",
            results.join("\n---\n")
        ))
    }
}
