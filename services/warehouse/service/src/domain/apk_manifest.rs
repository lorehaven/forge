//! Derives package identity server-side from an APK's `AndroidManifest.xml` -
//! a caller-supplied name/version can't be trusted. `attr` checks namespaced and bare forms.

use quench_convert::{axml, xml};
use quench_serde::Value;
use std::io::{Read, Seek};

/// The one file inside the archive this module looks at.
const MANIFEST_ENTRY: &str = "AndroidManifest.xml";

/// A compiled manifest is a few KiB; this only stops a hostile archive claiming gigabytes.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;

/// Checks the `android:`-namespaced form first, then the bare name.
fn attr<'a>(element: &'a Value, name: &str) -> Option<&'a str> {
    xml::attr(element, &format!("android:{name}")).or_else(|| xml::attr(element, name))
}

/// Marks an unresolved resource reference (e.g. `@string/app_name`), which has no text to show.
const UNRESOLVED_REFERENCE_PREFIX: &str = "@";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApkMetadata {
    pub package_name: String,
    pub version_code: i64,
    pub version_name: String,
    pub min_sdk_version: Option<i32>,
    pub target_sdk_version: Option<i32>,
    /// `None` for an unresolvable resource reference or a missing `<application>`.
    pub label: Option<String>,
    pub permissions: Vec<String>,
}

#[derive(Debug, quench_core::Error)]
pub enum ApkManifestError {
    #[error("not a valid zip archive")]
    NotAZip,
    #[error("no AndroidManifest.xml entry in the archive")]
    MissingManifest,
    #[error("AndroidManifest.xml could not be decoded: {0}")]
    InvalidManifest(String),
    #[error("manifest has no `package` attribute")]
    MissingPackage,
    #[error("manifest has no `versionCode` attribute, or it is not an integer")]
    MissingVersionCode,
}

/// Extracts package identity. Takes a seekable reader (not bytes) so a flushed upload `File`
/// doesn't need re-buffering; `zip` needs random access for the central directory either way.
pub fn extract<R: Read + Seek>(reader: R) -> Result<ApkMetadata, ApkManifestError> {
    let mut archive =
        quench_archive::zip::Archive::new(reader).map_err(|_| ApkManifestError::NotAZip)?;

    let index = archive
        .index_of(MANIFEST_ENTRY)
        .ok_or(ApkManifestError::MissingManifest)?;
    let manifest_bytes = archive
        .read(index, MAX_MANIFEST_BYTES)
        .map_err(|e| ApkManifestError::InvalidManifest(e.to_string()))?;

    let manifest = axml::parse(&manifest_bytes)
        .map_err(|e| ApkManifestError::InvalidManifest(e.to_string()))?;

    let package_name = xml::attr(&manifest, "package")
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or(ApkManifestError::MissingPackage)?;

    let version_code = attr(&manifest, "versionCode")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or(ApkManifestError::MissingVersionCode)?;

    let version_name = attr(&manifest, "versionName")
        .unwrap_or_default()
        .to_string();

    let mut min_sdk_version = None;
    let mut target_sdk_version = None;
    let mut label = None;
    let mut permissions = Vec::new();

    for element in xml::children(&manifest) {
        match xml::tag(element) {
            Some("uses-sdk") => {
                min_sdk_version = attr(element, "minSdkVersion").and_then(|v| v.parse().ok());
                target_sdk_version = attr(element, "targetSdkVersion").and_then(|v| v.parse().ok());
            }
            Some("application") => {
                label = attr(element, "label")
                    .filter(|value| !value.starts_with(UNRESOLVED_REFERENCE_PREFIX))
                    .map(str::to_string);
            }
            Some("uses-permission" | "uses-permission-sdk-23") => {
                if let Some(name) = attr(element, "name") {
                    permissions.push(name.to_string());
                }
            }
            _ => {}
        }
    }

    Ok(ApkMetadata {
        package_name,
        version_code,
        version_name,
        min_sdk_version,
        target_sdk_version,
        label,
        permissions,
    })
}
