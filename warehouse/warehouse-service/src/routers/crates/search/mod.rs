use crate::routers::crates::CRATES_STORAGE_ROOT;
use actix_web::{HttpResponse, Responder, get, web};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Search query string (matches against crate name)
    q: String,
    /// Results per page (1–100, default 10)
    #[serde(default = "default_per_page")]
    per_page: usize,
    /// Page number (1-based, default 1)
    #[serde(default = "default_page")]
    page: usize,
}

fn default_per_page() -> usize {
    10
}
fn default_page() -> usize {
    1
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct SearchCrate {
    name: String,
    max_version: String,
    description: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SearchMeta {
    total: usize,
}

#[derive(Serialize, ToSchema)]
pub struct SearchResponse {
    crates: Vec<SearchCrate>,
    meta: SearchMeta,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    operation_id = "search_crates",
    tags = ["crates - search"],
    path = "",
    params(
        ("q"        = String,          Query, description = "Search query string"),
        ("per_page" = Option<usize>,   Query, description = "Results per page (max 100, default 10)"),
        ("page"     = Option<usize>,   Query, description = "Page number (default 1)"),
    ),
    responses(
        (status = 200, description = "Search results", body = SearchResponse, content_type = "application/json"),
        (status = 400, description = "Bad request"),
        (status = 429, description = "Too many requests"),
    )
)]
#[get("")]
pub async fn handle(query: web::Query<SearchQuery>) -> impl Responder {
    let q = query.q.trim().to_ascii_lowercase();

    if q.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "errors": [{ "detail": "search query must not be empty" }]
        }));
    }

    let per_page = query.per_page.clamp(1, 100);
    let page = query.page.max(1);

    // Collect all matching crates by scanning the storage directory.
    // Each crate lives at <root>/<name>/ ; we match names containing `q`.
    let crate_root = std::path::PathBuf::from(CRATES_STORAGE_ROOT.as_str());
    let mut matches: Vec<SearchCrate> = Vec::new();

    if let Ok(mut entries) = tokio::fs::read_dir(&crate_root).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy().to_ascii_lowercase();

            // Skip the index directory
            if name == "index" {
                continue;
            }

            if !name.contains(q.as_str()) {
                continue;
            }

            // The max version is the lexicographically greatest version
            // sub-directory present — good enough for a private registry.
            let max_version = find_max_version(&entry.path()).await;

            if let Some(version) = max_version {
                matches.push(SearchCrate {
                    name: name.clone(),
                    max_version: version,
                    description: None, // We don't store description separately
                });
            }
        }
    }

    // Sort alphabetically for stable results
    matches.sort_by(|a, b| a.name.cmp(&b.name));

    let total = matches.len();
    let offset = (page - 1) * per_page;
    let page_results: Vec<SearchCrate> = matches.into_iter().skip(offset).take(per_page).collect();

    HttpResponse::Ok().json(SearchResponse {
        crates: page_results,
        meta: SearchMeta { total },
    })
}

// ---------------------------------------------------------------------------
// Helper: find the highest version sub-directory under a crate directory
// ---------------------------------------------------------------------------

async fn find_max_version(crate_dir: &std::path::Path) -> Option<String> {
    let mut versions: Vec<String> = Vec::new();

    let mut entries = tokio::fs::read_dir(crate_dir).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let meta = entry.metadata().await;
        if meta.map(|m| m.is_dir()).unwrap_or(false) {
            versions.push(entry.file_name().to_string_lossy().into_owned());
        }
    }

    if versions.is_empty() {
        return None;
    }

    // Use semver parsing when available; fall back to lexicographic order.
    versions.sort_by(|a, b| compare_versions(a, b));
    versions.into_iter().last()
}

/// Compares two version strings using semver semantics when both parse
/// successfully, otherwise falls back to lexicographic comparison.
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    match (parse_semver(a), parse_semver(b)) {
        (Some(av), Some(bv)) => av.cmp(&bv),
        _ => a.cmp(b),
    }
}

/// Parses a `major.minor.patch[-pre][+build]` string into a comparable tuple.
/// Returns `None` for anything that doesn't fit the pattern.
fn parse_semver(v: &str) -> Option<(u64, u64, u64, String)> {
    // Strip build metadata before parsing
    let v = v.split('+').next()?;
    let (numeric, pre) = if let Some(idx) = v.find('-') {
        (&v[..idx], v[idx + 1..].to_string())
    } else {
        (v, String::new())
    };
    let mut parts = numeric.split('.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next()?.parse().ok()?;
    let patch: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None; // too many numeric segments
    }
    // Pre-release versions sort before the release; encode absence as high value
    let pre_sort = if pre.is_empty() {
        "\u{FFFF}".to_string() // sorts after any pre-release string
    } else {
        pre
    };
    Some((major, minor, patch, pre_sort))
}
