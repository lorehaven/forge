//! Derives package identity server-side from an APK's `AndroidManifest.xml` -
//! a caller-supplied name/version can't be trusted. `attr` checks namespaced and bare forms.

use axmldecoder::{Element, Node};
use std::io::{Read, Seek};

/// The one file inside the archive this module looks at.
const MANIFEST_ENTRY: &str = "AndroidManifest.xml";

/// Checks the `android:`-namespaced form first, then the bare name.
fn attr<'a>(element: &'a Element, name: &str) -> Option<&'a String> {
    element
        .get_attributes()
        .get(&format!("android:{name}"))
        .or_else(|| element.get_attributes().get(name))
}

/// Marks an unresolved resource reference (e.g. `@string/app_name`) `axmldecoder` can't decode to text.
const UNRESOLVED_REFERENCE_PREFIX: &str = "ResourceValueType::";

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

#[derive(Debug, thiserror::Error)]
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
    let mut archive = zip::ZipArchive::new(reader).map_err(|_| ApkManifestError::NotAZip)?;

    let manifest_bytes = {
        let mut manifest_file = archive
            .by_name(MANIFEST_ENTRY)
            .map_err(|_| ApkManifestError::MissingManifest)?;
        let mut buf = Vec::with_capacity(manifest_file.size() as usize);
        manifest_file
            .read_to_end(&mut buf)
            .map_err(|e| ApkManifestError::InvalidManifest(e.to_string()))?;
        buf
    };

    let document = axmldecoder::parse(&manifest_bytes)
        .map_err(|e| ApkManifestError::InvalidManifest(e.to_string()))?;

    let Some(Node::Element(manifest)) = document.get_root() else {
        return Err(ApkManifestError::InvalidManifest(
            "document has no root element".to_string(),
        ));
    };

    let package_name = manifest
        .get_attributes()
        .get("package")
        .filter(|name| !name.is_empty())
        .cloned()
        .ok_or(ApkManifestError::MissingPackage)?;

    let version_code = attr(manifest, "versionCode")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or(ApkManifestError::MissingVersionCode)?;

    let version_name = attr(manifest, "versionName").cloned().unwrap_or_default();

    let mut min_sdk_version = None;
    let mut target_sdk_version = None;
    let mut label = None;
    let mut permissions = Vec::new();

    for child in manifest.get_children() {
        let Node::Element(element) = child else {
            continue;
        };

        match element.get_tag() {
            "uses-sdk" => {
                min_sdk_version = attr(element, "minSdkVersion").and_then(|v| v.parse().ok());
                target_sdk_version = attr(element, "targetSdkVersion").and_then(|v| v.parse().ok());
            }
            "application" => {
                label = attr(element, "label")
                    .filter(|value| !value.starts_with(UNRESOLVED_REFERENCE_PREFIX))
                    .cloned();
            }
            "uses-permission" | "uses-permission-sdk-23" => {
                if let Some(name) = attr(element, "name") {
                    permissions.push(name.clone());
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
