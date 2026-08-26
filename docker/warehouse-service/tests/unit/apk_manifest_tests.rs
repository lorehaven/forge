//! Fixtures under `tests/fixtures/apk/` are real APKs built with the Android
//! SDK's `aapt` (`aapt package -M AndroidManifest.xml ... -I android.jar`),
//! not hand-crafted bytes - the compiled binary XML format has enough
//! internal bookkeeping (string pool offsets, a resource-ID map) that a
//! hand-rolled fixture would risk testing this module against its own
//! assumptions about the format rather than the format itself.
//!
//! - `minimal.apk`: package/version/sdk/permissions/a literal `label`.
//! - `resource_label.apk`: `label` is `@string/app_name` - a resource
//!   reference axmldecoder can't resolve without `resources.arsc`.
//! - `missing_version_code.apk`: no `android:versionCode` on `<manifest>`.
//! - `no_manifest.zip`: a plain zip with no `AndroidManifest.xml` entry.

use std::io::Cursor;
use warehouse_service::domain::apk_manifest::{ApkManifestError, extract};

const MINIMAL: &[u8] = include_bytes!("../fixtures/apk/minimal.apk");
const RESOURCE_LABEL: &[u8] = include_bytes!("../fixtures/apk/resource_label.apk");
const MISSING_VERSION_CODE: &[u8] = include_bytes!("../fixtures/apk/missing_version_code.apk");
const NO_MANIFEST: &[u8] = include_bytes!("../fixtures/apk/no_manifest.zip");

#[test]
fn extracts_identity_and_sdk_levels_from_a_real_manifest() {
    let metadata = extract(Cursor::new(MINIMAL)).expect("valid manifest");

    assert_eq!(metadata.package_name, "com.example.forge.testapp");
    assert_eq!(metadata.version_code, 7);
    assert_eq!(metadata.version_name, "1.2.3");
    assert_eq!(metadata.min_sdk_version, Some(21));
    assert_eq!(metadata.target_sdk_version, Some(34));
    assert_eq!(metadata.label.as_deref(), Some("Forge Test App"));
}

#[test]
fn collects_every_uses_permission() {
    let metadata = extract(Cursor::new(MINIMAL)).expect("valid manifest");

    assert_eq!(
        metadata.permissions,
        vec![
            "android.permission.INTERNET".to_string(),
            "android.permission.ACCESS_NETWORK_STATE".to_string(),
        ]
    );
}

#[test]
fn label_is_none_when_it_is_an_unresolved_resource_reference() {
    let metadata = extract(Cursor::new(RESOURCE_LABEL)).expect("valid manifest");

    assert_eq!(metadata.package_name, "com.example.forge.testapp");
    assert_eq!(metadata.version_code, 9);
    assert_eq!(metadata.label, None);
}

#[test]
fn missing_version_code_is_rejected() {
    let err = extract(Cursor::new(MISSING_VERSION_CODE)).unwrap_err();
    assert!(matches!(err, ApkManifestError::MissingVersionCode));
}

#[test]
fn zip_with_no_manifest_entry_is_rejected() {
    let err = extract(Cursor::new(NO_MANIFEST)).unwrap_err();
    assert!(matches!(err, ApkManifestError::MissingManifest));
}

#[test]
fn non_zip_bytes_are_rejected() {
    let err = extract(Cursor::new(b"not a zip at all".as_slice())).unwrap_err();
    assert!(matches!(err, ApkManifestError::NotAZip));
}
