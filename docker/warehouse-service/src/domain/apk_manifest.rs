//! Deriving package identity from an APK's own `AndroidManifest.xml`.
//!
//! An app store cannot trust whatever a publisher *claims* a package's name
//! and version are - two APKs uploaded under the same "name" a client picked
//! would silently overwrite each other's history, or worse, let an attacker
//! ship an update to someone else's listing. The manifest that Android itself
//! reads at install time is the one fact that can't be faked without also
//! rebuilding the archive, so publish-time identity comes from decoding it
//! server-side, not from a caller-supplied field.
//!
//! `AndroidManifest.xml` inside a built APK is not text - it is a compiled
//! binary format. Both `aapt` and `aapt2` (verified against the fixtures
//! under `tests/fixtures/apk/`) write the `android:` attributes this module
//! reads (`versionCode`, `versionName`, `minSdkVersion`, `targetSdkVersion`,
//! `label`, `uses-permission`'s `name`) as an explicit namespaced name in
//! [`axmldecoder`]'s output, not resolved down to a plain `versionCode` the
//! way its own resource-string table (built for a different encoding some
//! other toolchain apparently produces) would suggest - [`attr`] checks the
//! namespaced form first and falls back to the bare name so either encoding
//! works. `package` is an ordinary unnamespaced attribute and comes through
//! as-is either way.

use axmldecoder::{Element, Node};
use std::io::{Read, Seek};

/// The one file inside the archive this module looks at.
const MANIFEST_ENTRY: &str = "AndroidManifest.xml";

/// Looks up an `android:`-namespaced attribute, falling back to the bare
/// name - see this module's doc comment for why both forms are checked.
fn attr<'a>(element: &'a Element, name: &str) -> Option<&'a String> {
    element
        .get_attributes()
        .get(&format!("android:{name}"))
        .or_else(|| element.get_attributes().get(name))
}

/// A prefix [`axmldecoder`] renders an attribute value as when it is a
/// resource reference (`@string/app_name`, say) rather than a literal -
/// something that can't be resolved to text without also parsing
/// `resources.arsc`, which the decoder deliberately doesn't support. See
/// `ResourceValue::get_value`'s fallback arm in `axmldecoder`'s source.
const UNRESOLVED_REFERENCE_PREFIX: &str = "ResourceValueType::";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApkMetadata {
    pub package_name: String,
    pub version_code: i64,
    pub version_name: String,
    pub min_sdk_version: Option<i32>,
    pub target_sdk_version: Option<i32>,
    /// `None` when the manifest's `label` is a resource reference this
    /// module can't resolve to text, or when there is no `<application>`.
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

/// Extracts package identity from an APK's `AndroidManifest.xml`.
///
/// Takes a seekable reader rather than a byte slice so a caller can hand it
/// an open `File` for an upload already flushed to disk instead of buffering
/// the whole archive in memory a second time - `zip` needs random access to
/// read the central directory regardless of how the entry itself is stored.
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
