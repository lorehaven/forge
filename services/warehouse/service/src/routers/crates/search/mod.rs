use crate::routers::crates::crates_storage_root;
use quench_http::prelude::{Query, Response, get, http::StatusCode};
use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    q: String,
    /// Only crates that build a binary, each listing its binary names; allows an empty `q` (list them all).
    #[serde(default)]
    bins: bool,
    #[serde(default = "default_per_page")]
    per_page: usize,
    #[serde(default = "default_page")]
    page: usize,
}

const fn default_per_page() -> usize {
    10
}
const fn default_page() -> usize {
    1
}

#[derive(Debug, Serialize)]
pub struct SearchCrate {
    name: String,
    max_version: String,
    description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    bins: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchMeta {
    total: usize,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    crates: Vec<SearchCrate>,
    meta: SearchMeta,
}

#[get("/api/v1/crates")]
pub async fn handle(Query(query): Query<SearchQuery>) -> Response {
    let q = query.q.trim().to_ascii_lowercase();

    if q.is_empty() && !query.bins {
        return Response::json(
            StatusCode::BAD_REQUEST,
            &quench_serde::json!({ "errors": [{ "detail": "search query must not be empty" }] }),
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
                let bins = if query.bins {
                    binaries_of(&entry.path(), &name, &version).await
                } else {
                    Vec::new()
                };
                if query.bins && bins.is_empty() {
                    continue;
                }
                matches.push(SearchCrate {
                    name: name.clone(),
                    max_version: version,
                    description: None,
                    bins,
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

/// The binary names a published version builds, from the `[[bin]]` tables of the `Cargo.toml` inside its `.crate`.
///
/// The answer is cached beside the crate as `bins.json`, so a version is unpacked once; crates published before this
/// existed are read the first time they are asked for, with no republish. A crate with no `[[bin]]` table (or an
/// unreadable archive) has none.
pub async fn binaries_of(crate_dir: &std::path::Path, name: &str, version: &str) -> Vec<String> {
    let version_dir = crate_dir.join(version);
    let cache = version_dir.join("bins.json");
    if let Ok(text) = tokio::fs::read_to_string(&cache).await
        && let Ok(bins) = quench_serde::json::from_str::<Vec<String>>(&text)
    {
        return bins;
    }
    let Ok(archive) = tokio::fs::read(version_dir.join(format!("{name}-{version}.crate"))).await else {
        return Vec::new();
    };
    let bins = binaries_in_archive(&archive, name, version);
    let _ = tokio::fs::write(&cache, quench_serde::json::to_string(&bins)).await;
    bins
}

/// [`binaries_of`] on the bytes of a `.crate` (a gzip tar holding `<name>-<version>/Cargo.toml`).
#[must_use]
pub fn binaries_in_archive(archive: &[u8], name: &str, version: &str) -> Vec<String> {
    const LIMIT: usize = 512 * 1024 * 1024;
    let Ok(tar) = quench_archive::gzip::decompress(archive, LIMIT) else {
        return Vec::new();
    };
    let manifest = format!("{name}-{version}/Cargo.toml");
    for entry in quench_archive::tar::Reader::new(&tar).flatten() {
        if entry.path == manifest {
            return binaries_in_manifest(&String::from_utf8_lossy(entry.data));
        }
    }
    Vec::new()
}

/// The `name` of every `[[bin]]` table in a manifest, in order.
#[must_use]
pub fn binaries_in_manifest(manifest: &str) -> Vec<String> {
    let mut bins = Vec::new();
    let mut in_bin = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_bin = line == "[[bin]]";
        } else if in_bin
            && let Some(rest) = line.strip_prefix("name")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            let value = value.trim().trim_matches('"');
            if !value.is_empty() {
                bins.push(value.to_string());
            }
        }
    }
    bins
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
        if meta.is_ok_and(|m| m.is_dir()) {
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
#[must_use]
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    match (parse_semver(a), parse_semver(b)) {
        (Some(av), Some(bv)) => av.cmp(&bv),
        _ => a.cmp(b),
    }
}

/// Parses `major.minor.patch[-pre][+build]` into a comparable tuple; `None` if it doesn't fit.
#[must_use]
pub fn parse_semver(v: &str) -> Option<(u64, u64, u64, String)> {
    let v = v.split('+').next()?; // strip build metadata
    let (numeric, pre) = v.find('-').map_or_else(
        || (v, String::new()),
        |idx| (&v[..idx], v[idx + 1..].to_string()),
    );
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
