//! The artifact catalog: one row per published build of a program on one
//! platform, keyed by `<program>/<platform>@<version_code>`.
//!
//! `program` is the reverse-DNS application id shared across platforms
//! (`dev.lorehaven.pedlar` on Android, Linux and Windows alike); `platform`
//! is the tag a client filters on. For `android`, `version_code` and
//! `version_name` are decoded from the APK's own `AndroidManifest.xml` at
//! publish time (see [`super::apk_manifest`]) and the publish is rejected if
//! they don't match the URL - so an app-store consumer can trust the catalog.
//! For every other platform there is no manifest to decode, so identity is
//! taken from the publish URL as asserted by the caller.
//!
//! Like the old `apk_versions` table this replaces, and unlike the
//! dynamic-storage tables in [`super::storage`] and [`super::storage_file`],
//! nothing here needs a locked read-then-write - an artifact version is
//! immutable once published (yanking flips a flag, it doesn't rewrite
//! content) - so this goes through `quench-db`'s generic
//! [`quench_db::prelude::Crud`] via a [`quench_db::prelude::Repository`]
//! instead of hand-written SQL.
//!
//! `created_at` and `yanked` are set by the caller rather than left to the
//! column defaults: `Crud::create` populates every column from the model's
//! serialized JSON (`jsonb_populate_record`), and a key that JSON omits or
//! sets to `null` overrides a `DEFAULT` with `NULL` rather than leaving it
//! alone - so a field with a `NOT NULL` column has to be filled in here, not
//! trusted to Postgres.

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

    /// The tag as it appears in a URL or the `platform` column, or `None` for
    /// anything not in [`Platform::ALL`].
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "android" => Some(Platform::Android),
            "linux" => Some(Platform::Linux),
            "windows" => Some(Platform::Windows),
            "macos" => Some(Platform::Macos),
            _ => None,
        }
    }

    /// Whether identity (`program`, `version_code`, `version_name`) is proven
    /// from the uploaded file rather than trusted from the URL. Only Android
    /// carries a manifest this service can decode.
    pub fn identity_is_verifiable(self) -> bool {
        matches!(self, Platform::Android)
    }
}

/// Platform-specific extras kept out of the top-level columns so a Linux or
/// Windows row isn't carrying a column-per-Android-concept it never uses.
/// Absent keys deserialize to their empty value, so an older row or a
/// non-Android row round-trips fine.
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
    /// One of [`Platform::as_str`]. Stored as text (the DB has a `CHECK`), and
    /// validated at the router boundary before it ever reaches here.
    pub platform: String,
    /// `x86_64` / `aarch64` / `universal`; `None` for Android, where the APK
    /// is a fat archive.
    pub arch: Option<String>,
    /// `apk`, `tar.gz`, `zip`, `AppImage`, `deb`, `msi`, `exe`, … - a tag the
    /// client echoes, never inspected server-side for anything but Android.
    pub format: String,
    pub version_code: i64,
    pub version_name: String,
    /// The name the download is served as, e.g. `pedlar-7.tar.gz`.
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
