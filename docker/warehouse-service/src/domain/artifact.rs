//! The artifact catalog: one row per published build, keyed by `<program>/<platform>@<version_code>`.
//! Immutable once published (yanking just flips a flag), so plain `Crud` suffices.

use chrono::{DateTime, Utc};
use quench_db::prelude::Model;
use sqlx::types::Json;

/// A platform an artifact can target. Serialises to the lowercase tag stored
/// in the `platform` column and accepted in the `{platform}` path segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Android,
    Linux,
    Windows,
    Macos,
}

impl Platform {
    /// Every platform, for tests and exhaustive UI listing.
    pub const ALL: [Platform; 4] = [
        Platform::Android,
        Platform::Linux,
        Platform::Windows,
        Platform::Macos,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Android => "android",
            Platform::Linux => "linux",
            Platform::Windows => "windows",
            Platform::Macos => "macos",
        }
    }

    /// `None` for anything not in [`Platform::ALL`].
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "android" => Some(Platform::Android),
            "linux" => Some(Platform::Linux),
            "windows" => Some(Platform::Windows),
            "macos" => Some(Platform::Macos),
            _ => None,
        }
    }

    /// Whether identity is proven from the file rather than trusted from the URL (Android only).
    pub fn identity_is_verifiable(self) -> bool {
        matches!(self, Platform::Android)
    }
}

/// Platform-specific extras, kept out of the top-level columns. Absent keys default empty.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct ArtifactMetadata {
    #[serde(default)]
    pub min_sdk_version: Option<i32>,
    #[serde(default)]
    pub target_sdk_version: Option<i32>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
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
    pub metadata: Json<ArtifactMetadata>,
    pub uploaded_by: String,
    pub yanked: bool,
    pub created_at: DateTime<Utc>,
}

impl ArtifactVersion {
    /// The catalog key a caller addresses a version by.
    pub fn id_for(program: &str, platform: Platform, version_code: i64) -> String {
        format!("{program}/{}@{version_code}", platform.as_str())
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
