#![warn(missing_docs)]
//! The `.rivet` package: a whole overlay, versioned and checksummed, as a `tar.zst`.
//!
//! Nothing in here touches the network or the database. It parses and validates
//! the manifest, builds a deterministic archive, and reads one back with every
//! check a registry needs before it will store the bytes: no path that escapes,
//! no link or device entries, bounded size, and every file matching `SHA256SUMS`.
//!
//! A crate of its own so `warehouse-service` (which validates a publish) and
//! `riveter` (which builds and installs packages) share one definition of the
//! format rather than two that agree most of the time.

pub mod archive;
pub mod error;
pub mod manifest;
pub mod path;

pub use archive::{Limits, Package, PackageBuilder};
pub use error::PackageError;
pub use manifest::{Manifest, PackageMeta, Requires};

/// Extension of a package file, without the dot.
pub const EXTENSION: &str = "rivet";
/// The manifest's fixed name inside the archive.
pub const MANIFEST_FILE: &str = "rivet.toml";
/// The overlay entry point, which every package must carry.
pub const OVERLAY_FILE: &str = "overlay.yaml";
/// Per-file digests; covers every file except itself.
pub const SUMS_FILE: &str = "SHA256SUMS";
/// Optional default values, overridable at install time.
pub const VALUES_FILE: &str = "values.toml";

/// Hex SHA-256 of `data`.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(data))
}

/// The file name a package is stored and downloaded as: `<name>-<version>.rivet`.
#[must_use]
pub fn file_name(name: &str, version: &str) -> String {
    format!("{name}-{version}.{EXTENSION}")
}
