//! The public catalogue (served by hoist-service: F-Droid, IzzyOnDroid and Aptoide merged): what it lists, how the home page picks
//! from it, and how its categories are shown.

use quench_serde::Deserialize;

/// One app of the public catalogue, as `GET /hoist/v1/apps` returns it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PublicApp {
    pub package_name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    pub version_code: i64,
    #[serde(default)]
    pub version_name: String,
    #[serde(default)]
    pub size_bytes: i64,
    pub download_url: String,
    /// Present for F-Droid and IzzyOnDroid builds; Aptoide's may have none.
    #[serde(default)]
    pub sha256: Option<String>,
    /// `fdroid`, `izzyondroid` or `aptoide`.
    #[serde(default)]
    pub install_source: String,
    #[serde(default)]
    pub rating_avg: Option<f64>,
    #[serde(default)]
    pub rating_count: Option<i64>,
    #[serde(default)]
    pub downloads: Option<i64>,
}

impl PublicApp {
    /// What a person sees: the label, else the package.
    #[must_use]
    pub fn title(&self) -> &str {
        if self.label.is_empty() {
            &self.package_name
        } else {
            &self.label
        }
    }

    /// Whether the download carries a checksum hoist can check.
    #[must_use]
    pub fn is_verifiable(&self) -> bool {
        self.sha256.as_deref().is_some_and(|h| !h.is_empty())
    }

    /// `★ 4.3 (120)`, or nothing when nobody rated it.
    #[must_use]
    pub fn rating_text(&self) -> String {
        match (self.rating_avg, self.rating_count) {
            (Some(avg), Some(n)) if n > 0 => format!("\u{2605} {avg:.1} ({n})"),
            (Some(avg), _) if avg > 0.0 => format!("\u{2605} {avg:.1}"),
            _ => String::new(),
        }
    }
}

/// One page of the catalogue.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PublicPage {
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub offset: i64,
    pub items: Vec<PublicApp>,
}

impl PublicPage {
    /// Whether there is more after this page.
    #[must_use]
    pub fn has_more(&self) -> bool {
        self.offset + (self.items.len() as i64) < self.total
    }
}

/// A category and how many apps it holds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Category {
    pub name: String,
    #[serde(default)]
    pub count: i64,
}

/// What a request for a page of the catalogue asks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PublicQuery {
    pub query: Option<String>,
    pub category: Option<String>,
    /// `downloads`, `rating` or `alpha`.
    pub sort: String,
    /// Fewest ratings an app must have (a high average from two votes is noise).
    pub min_votes: Option<i64>,
    pub offset: i64,
    pub limit: i64,
}

/// The categories the home page shows: the biggest ones that have a name, at most `n`.
#[must_use]
pub fn home_categories(all: &[Category], n: usize) -> Vec<Category> {
    all.iter()
        .filter(|c| !c.name.trim().is_empty() && c.count > 0)
        .take(n)
        .cloned()
        .collect()
}

/// What a section of the home page lists: the first `n` of `apps` (best rated first, as the server sorted them)
/// that are not installed.
#[must_use]
pub fn suggestions(
    apps: &[PublicApp],
    installed: &dyn Fn(&str) -> bool,
    n: usize,
) -> Vec<PublicApp> {
    apps.iter()
        .filter(|a| !installed(&a.package_name))
        .take(n)
        .cloned()
        .collect()
}
