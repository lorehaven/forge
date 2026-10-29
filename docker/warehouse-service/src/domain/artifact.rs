//! The artifact catalog: one row per published build, keyed by `<program>/<platform>/<format>@<version_code>`.
//!
//! One version of a program on one platform can ship in several formats (a `.deb`, an `.rpm`, an Arch package).
//! Immutable once published (yanking just flips a flag), so plain `Crud` suffices.

use quench_core::time::Time;
use quench_db::prelude::Model;

/// A platform an artifact can target. Serialises to the lowercase tag stored
/// in the `platform` column and accepted in the `{platform}` path segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, quench_serde::Serialize, quench_serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Android,
    Linux,
    Windows,
    Macos,
}

impl Platform {
    /// Every platform, for tests and exhaustive UI listing.
    pub const ALL: [Self; 4] = [Self::Android, Self::Linux, Self::Windows, Self::Macos];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Linux => "linux",
            Self::Windows => "windows",
            Self::Macos => "macos",
        }
    }

    /// `None` for anything not in [`Platform::ALL`].
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "android" => Some(Self::Android),
            "linux" => Some(Self::Linux),
            "windows" => Some(Self::Windows),
            "macos" => Some(Self::Macos),
            _ => None,
        }
    }

    /// Whether identity is proven from the file rather than trusted from the URL (Android only).
    #[must_use]
    pub const fn identity_is_verifiable(self) -> bool {
        matches!(self, Self::Android)
    }
}

/// Platform-specific extras, kept out of the top-level columns. Absent keys default empty.
#[derive(
    Debug, Clone, PartialEq, Eq, Default, quench_serde::Serialize, quench_serde::Deserialize,
)]
pub struct ArtifactMetadata {
    #[serde(default)]
    pub min_sdk_version: Option<i32>,
    #[serde(default)]
    pub target_sdk_version: Option<i32>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, quench_serde::Serialize, quench_serde::Deserialize)]
pub struct ArtifactVersion {
    pub id: String,
    pub program: String,
    /// One of [`Platform::as_str`]; validated at the router boundary.
    pub platform: String,
    /// `x86_64` / `aarch64` / `universal`; `None` for Android's fat APK.
    pub arch: Option<String>,
    /// `apk`, `tar.gz`, `zip`, … - a client-echoed tag, unchecked outside Android.
    pub format: String,
    pub version_code: i64,
    pub version_name: String,
    /// The name the download is served as.
    pub filename: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub label: Option<String>,
    pub metadata: ArtifactMetadata,
    pub uploaded_by: String,
    pub yanked: bool,
    pub created_at: Time,
}

impl ArtifactVersion {
    /// The catalog key a caller addresses a version by.
    #[must_use]
    pub fn id_for(program: &str, platform: Platform, format: &str, version_code: i64) -> String {
        format!("{program}/{}/{format}@{version_code}", platform.as_str())
    }
}

impl Model for ArtifactVersion {
    fn table_name() -> String {
        format!("{}.artifact_versions", crate::domain::db::schema())
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "id",
            "program",
            "platform",
            "arch",
            "format",
            "version_code",
            "version_name",
            "filename",
            "size_bytes",
            "sha256",
            "label",
            "metadata",
            "uploaded_by",
            "yanked",
            "created_at",
        ]
    }
}
