//! Shared shapes between connectors, the merge step, and the DB/API layer.
//!
//! [`RawListing`] is what a connector hands back — one per (source,
//! package) before any dedup. [`MergedPackage`] is the one-row-per-package
//! result after `merge::merge_all` — what's actually stored and served.

/// One catalog source's offer for one package, before dedup.
///
/// A listing may not have a resolved download yet (an Aptoide browse row
/// before its post-dedup `getMeta` resolution) — `merge::merge_all` drops
/// any package that still has no resolved listing anywhere by the time it
/// picks a per-package winner.
#[derive(Debug, Clone, Default)]
pub struct RawListing {
    pub source: String, // "fdroid" | "izzyondroid" | "aptoide"
    pub package_name: String,
    pub label: String,
    pub version_code: i64,
    pub version_name: String,
    pub size_bytes: i64,

    pub summary: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub category: Option<String>,

    pub download_url: Option<String>,
    pub sha256: Option<String>,
    pub md5: Option<String>,

    pub rating_avg: Option<f64>,
    pub rating_count: Option<i64>,
    pub downloads: Option<i64>,

    /// Aptoide-only: its own malware-scan verdict, used to rank
    /// same-package listings from different stores against each other
    /// (see `merge::dedupe_within_source`).
    pub malware_rank: Option<String>,
}

/// The one-row-per-package result of a merge — what's stored in SQLite and
/// served over the API.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MergedPackage {
    pub package_name: String,
    pub label: String,
    pub version_code: i64,
    pub version_name: String,
    pub size_bytes: i64,
    pub download_url: String,
    /// Which source's download_url/checksum this row uses.
    pub install_source: String,

    pub summary: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub category: Option<String>,
    pub sha256: Option<String>,
    pub md5: Option<String>,
    pub rating_avg: Option<f64>,
    pub rating_count: Option<i64>,
    pub downloads: Option<i64>,

    /// Aptoide's malware-scan verdict for whichever listing won the
    /// install slot — only ever set when `install_source == "aptoide"`
    /// (F-Droid and IzzyOnDroid builds are source-built, this concept
    /// doesn't apply to them).
    pub malware_rank: Option<String>,
}
