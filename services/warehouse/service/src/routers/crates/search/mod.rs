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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    categories: Vec<String>,
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
                let info = if query.bins {
                    manifest_info_of(&entry.path(), &name, &version).await
                } else {
                    ManifestInfo::default()
                };
                if query.bins && info.bins.is_empty() {
                    continue;
                }
                matches.push(SearchCrate {
                    name: name.clone(),
                    max_version: version,
                    description: None,
                    bins: info.bins,
                    categories: info.categories,
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

/// What a published version's manifest says about how it is used.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestInfo {
    /// The `name` of each `[[bin]]` table.
    pub bins: Vec<String>,
    /// The `categories` of `[package]`.
    pub categories: Vec<String>,
}

/// [`ManifestInfo`] of a published version, from the `Cargo.toml` inside its `.crate`.
///
/// The answer is cached beside the crate as `manifest-info.json`, so a version is unpacked once; crates published
/// before this existed are read the first time they are asked for, with no republish. An unreadable archive has
/// no binaries and no categories.
pub async fn manifest_info_of(
    crate_dir: &std::path::Path,
    name: &str,
    version: &str,
) -> ManifestInfo {
    let version_dir = crate_dir.join(version);
    let cache = version_dir.join("manifest-info.json");
    if let Ok(text) = tokio::fs::read_to_string(&cache).await
        && let Ok(info) = quench_serde::json::from_str::<ManifestInfo>(&text)
    {
        return info;
    }
    let Ok(archive) = tokio::fs::read(version_dir.join(format!("{name}-{version}.crate"))).await
    else {
        return ManifestInfo::default();
    };
    let info = manifest_info_in_archive(&archive, name, version);
    let _ = tokio::fs::write(&cache, quench_serde::json::to_string(&info)).await;
    info
}

/// [`manifest_info_of`] on the bytes of a `.crate` (a gzip tar holding `<name>-<version>/Cargo.toml`).
#[must_use]
pub fn manifest_info_in_archive(archive: &[u8], name: &str, version: &str) -> ManifestInfo {
    const LIMIT: usize = 512 * 1024 * 1024;
    let Ok(tar) = quench_archive::gzip::decompress(archive, LIMIT) else {
        return ManifestInfo::default();
    };
    let manifest = format!("{name}-{version}/Cargo.toml");
    for entry in quench_archive::tar::Reader::new(&tar).flatten() {
        if entry.path == manifest {
            return manifest_info(&String::from_utf8_lossy(entry.data));
        }
    }
    ManifestInfo::default()
}

/// The binaries and categories a manifest declares.
#[must_use]
pub fn manifest_info(manifest: &str) -> ManifestInfo {
    ManifestInfo {
        bins: binaries_in_manifest(manifest),
        categories: categories_in_manifest(manifest),
    }
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

/// The `categories = [...]` of `[package]`, on one line or spread over several.
#[must_use]
pub fn categories_in_manifest(manifest: &str) -> Vec<String> {
    let mut in_package = false;
    let mut collecting = false;
    let mut text = String::new();
    for line in manifest.lines().map(str::trim) {
        if collecting {
            text.push_str(line);
        } else if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package
            && let Some(rest) = line.strip_prefix("categories")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            collecting = true;
            text.push_str(value);
        }
        if collecting && text.contains(']') {
            break;
        }
    }
    text.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
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
