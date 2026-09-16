use crate::routers::crates::crates_storage_root;
use quench_http::prelude::{Query, Response, get, http::StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    q: String,
    #[serde(default = "default_per_page")]
    per_page: usize,
    #[serde(default = "default_page")]
    page: usize,
}

fn default_per_page() -> usize {
    10
}
fn default_page() -> usize {
    1
}

#[derive(Serialize)]
pub struct SearchCrate {
    name: String,
    max_version: String,
    description: Option<String>,
}

#[derive(Serialize)]
pub struct SearchMeta {
    total: usize,
}

#[derive(Serialize)]
pub struct SearchResponse {
    crates: Vec<SearchCrate>,
    meta: SearchMeta,
}

#[get("/api/v1/crates")]
pub async fn handle(Query(query): Query<SearchQuery>) -> Response {
    let q = query.q.trim().to_ascii_lowercase();

    if q.is_empty() {
        return Response::json(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({ "errors": [{ "detail": "search query must not be empty" }] }),
        )
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
    }

    let per_page = query.per_page.clamp(1, 100);
    let page = query.page.max(1);

    let crate_root = std::path::PathBuf::from(crates_storage_root());
    let mut matches: Vec<SearchCrate> = Vec::new();

    if let Ok(mut entries) = tokio::fs::read_dir(&crate_root).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy().to_ascii_lowercase();

            if name == "index" {
                continue;
            }

            if !name.contains(q.as_str()) {
                continue;
            }

            let max_version = find_max_version(&entry.path()).await;

            if let Some(version) = max_version {
                matches.push(SearchCrate {
                    name: name.clone(),
                    max_version: version,
                    description: None,
                });
            }
        }
    }

    matches.sort_by(|a, b| a.name.cmp(&b.name));

    let total = matches.len();
    let offset = (page - 1) * per_page;
    let page_results: Vec<SearchCrate> = matches.into_iter().skip(offset).take(per_page).collect();

    Response::json(
        StatusCode::OK,
        &SearchResponse {
            crates: page_results,
            meta: SearchMeta { total },
        },
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = handle as fn(_) -> _;
}

/// The highest version sub-directory under a crate directory.
pub async fn find_max_version(crate_dir: &std::path::Path) -> Option<String> {
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

    versions.sort_by(|a, b| compare_versions(a, b));
    versions.into_iter().last()
}

/// Compares by semver when both strings parse; otherwise falls back to lexicographic order.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    match (parse_semver(a), parse_semver(b)) {
        (Some(av), Some(bv)) => av.cmp(&bv),
        _ => a.cmp(b),
    }
}

/// Parses `major.minor.patch[-pre][+build]` into a comparable tuple; `None` if it doesn't fit.
pub fn parse_semver(v: &str) -> Option<(u64, u64, u64, String)> {
    let v = v.split('+').next()?; // strip build metadata
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
    // Absent pre-release sorts after any real one.
    let pre_sort = if pre.is_empty() {
        "\u{FFFF}".to_string()
    } else {
        pre
    };
    Some((major, minor, patch, pre_sort))
}
