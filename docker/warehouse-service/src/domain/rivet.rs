//! The rivet package catalog: one row per published `.rivet`, keyed by `<name>@<version>`.
//! Immutable once published (yanking just flips a flag), so plain `Crud` suffices.

use quench_core::time::Time;
use quench_db::prelude::Model;

#[derive(Debug, Clone, PartialEq, quench_serde::Serialize, quench_serde::Deserialize)]
pub struct RivetPackage {
    pub id: String,
    pub name: String,
    /// Full semver, build metadata included; verified against the archive's manifest at publish.
    pub version: String,
    pub description: Option<String>,
    pub namespace: Option<String>,
    /// The name the download is served as.
    pub filename: String,
    pub size_bytes: i64,
    pub sha256: String,
    /// The parsed `rivet.toml`, as the archive declared it.
    pub manifest: quench_serde::Value,
    pub uploaded_by: String,
    pub yanked: bool,
    pub created_at: Time,
}

impl RivetPackage {
    /// The catalog key a caller addresses a version by.
    #[must_use]
    pub fn id_for(name: &str, version: &str) -> String {
        format!("{name}@{version}")
    }

    /// The version as semver, or `None` for a row whose version somehow is not.
    #[must_use]
    pub fn semver(&self) -> Option<quench_core::version::Version> {
        quench_core::version::Version::parse(&self.version).ok()
    }
}

/// The highest non-yanked version by semver precedence (build metadata breaks ties, as the
/// `semver` crate orders it), or `None` if every version is yanked.
#[must_use]
pub fn latest_of(versions: &[RivetPackage]) -> Option<&RivetPackage> {
    versions
        .iter()
        .filter(|package| !package.yanked)
        .filter_map(|package| package.semver().map(|version| (version, package)))
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, package)| package)
}

/// Newest first, rows with an unparseable version last.
pub fn sort_newest_first(versions: &mut [RivetPackage]) {
    versions.sort_by_key(|a| std::cmp::Reverse(a.semver()));
}

impl Model for RivetPackage {
    fn table_name() -> String {
        format!("{}.rivet_packages", crate::domain::db::schema())
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "id",
            "name",
            "version",
            "description",
            "namespace",
            "filename",
            "size_bytes",
            "sha256",
            "manifest",
            "uploaded_by",
            "yanked",
            "created_at",
        ]
    }
}
