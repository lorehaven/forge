//! The JSON the management API speaks. Timestamps are already text: the browser only shows them.

use quench_serde::{Deserialize, Serialize};

/// Which registries this deployment serves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Features {
    pub docker: bool,
    pub crates: bool,
    pub files: bool,
    pub artifacts: bool,
    pub rivets: bool,
}

/// Who is asking, what they may change, and which registries exist.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overview {
    pub username: String,
    /// The blanket `warehouse:write` grant (or a wildcard role): the bar for the management controls.
    pub can_manage: bool,
    pub features: Features,
}

// ── Docker ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerTag {
    pub tag: String,
    pub digest: String,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

/// An image to delete: the manifest the digest names, and every tag that points at it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRef {
    pub repository: String,
    pub digest: String,
}

// ── Crates ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrateDep {
    pub name: String,
    pub req: String,
    pub optional: bool,
    /// `normal`, `dev` or `build`.
    pub kind: String,
    #[serde(default)]
    pub target: Option<String>,
    /// The real package name when the dependency is renamed.
    #[serde(default)]
    pub package: Option<String>,
}

impl CrateDep {
    /// `serde ^1 [optional] [target: cfg(unix)]`, as the old page wrote it.
    #[must_use]
    pub fn line(&self) -> String {
        let name = self.package.as_deref().map_or_else(
            || self.name.clone(),
            |package| format!("{package} (as {})", self.name),
        );
        let mut line = format!("{name} {}", self.req);
        if self.optional {
            line.push_str(" [optional]");
        }
        if let Some(target) = &self.target {
            line = format!("{line} [target: {target}]");
        }
        line
    }
}

/// One published version of a crate, from the registry's index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrateVersion {
    pub vers: String,
    pub yanked: bool,
    pub cksum: String,
    #[serde(default)]
    pub rust_version: Option<String>,
    #[serde(default)]
    pub links: Option<String>,
    /// Every feature name (the index's `features` and `features2`), sorted.
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub deps: Vec<CrateDep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrateRef {
    pub name: String,
    pub version: String,
}

// ── Artifacts ────────────────────────────────────────────────────────────────

/// One published build of a program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub program: String,
    pub platform: String,
    pub format: String,
    pub version_code: i64,
    pub version_name: String,
    pub filename: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub uploaded_by: String,
    pub yanked: bool,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub min_sdk_version: Option<i32>,
    #[serde(default)]
    pub target_sdk_version: Option<i32>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub program: String,
    pub platform: String,
    pub version_code: i64,
}

// ── Rivets ───────────────────────────────────────────────────────────────────

/// One published version of a rivet package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RivetEntry {
    pub name: String,
    pub version: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub uploaded_by: String,
    pub yanked: bool,
    /// `2026-10-29 10:00 UTC`.
    pub published: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub requires_riveter: Option<String>,
    #[serde(default)]
    pub requires_packages: Vec<String>,
    /// The package's own `[meta]` table.
    #[serde(default)]
    pub meta: Vec<MetaPair>,
}

/// One entry of a rivet package's `[meta]` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetaPair {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RivetRef {
    pub name: String,
    pub version: String,
}

// ── Files ────────────────────────────────────────────────────────────────────

/// A storage the operator configured: a directory, read-only here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticStorage {
    pub name: String,
    pub root: String,
}

/// A storage in the database, with a quota.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicStorage {
    pub name: String,
    pub owner: String,
    #[serde(default)]
    pub max_file_bytes: Option<i64>,
    pub quota_bytes: i64,
    pub used_bytes: i64,
    pub sync_enabled: bool,
    /// `2026-10-29 10:00 UTC`.
    pub created: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Storages {
    pub fixed: Vec<StaticStorage>,
    pub dynamic: Vec<DynamicStorage>,
}

impl Storages {
    #[must_use]
    pub fn dynamic_named(&self, name: &str) -> Option<&DynamicStorage> {
        self.dynamic.iter().find(|s| s.name == name)
    }

    #[must_use]
    pub fn fixed_named(&self, name: &str) -> Option<&StaticStorage> {
        self.fixed.iter().find(|s| s.name == name)
    }
}

/// A new database storage. Sizes are in bytes; the form's GiB and MiB are the draft's business.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewStorage {
    pub name: String,
    pub owner: String,
    /// `None`: the deployment's default quota.
    #[serde(default)]
    pub quota_bytes: Option<i64>,
    #[serde(default)]
    pub max_file_bytes: Option<i64>,
    pub sync_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageChanges {
    /// `None`: keep the quota.
    #[serde(default)]
    pub quota_bytes: Option<i64>,
    /// `None`: keep the limit (unless `clear_max_file`).
    #[serde(default)]
    pub max_file_bytes: Option<i64>,
    pub clear_max_file: bool,
    pub sync_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRef {
    pub storage: String,
    pub path: String,
}

/// A file of a storage, with the size when known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowseFile {
    pub path: String,
    #[serde(default)]
    pub size: Option<i64>,
}

/// A storage's files, as far as the caller may see them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Browse {
    pub storage: String,
    /// `readable`, `denied` (the caller may not read this storage) or `missing`.
    pub access: String,
    pub files: Vec<BrowseFile>,
    /// There are more files than the browser lists.
    pub truncated: bool,
}

impl Browse {
    pub const READABLE: &'static str = "readable";
    pub const DENIED: &'static str = "denied";
    pub const MISSING: &'static str = "missing";
}
