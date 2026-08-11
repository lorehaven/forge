//! Shared `cargo metadata` resolution.
//!
//! `cargo metadata` already resolves `version.workspace = true` (and any
//! other inheritance) into a concrete string, already knows each package's
//! real directory regardless of how deep or shallow it lives in the
//! workspace, and already reports a package's `publish` restriction
//! (`null` = unrestricted, `[]` = `publish = false`, `[registry, ...]` =
//! restricted to those). Reading those straight from cargo's own resolution
//! is what release/docker package lookups should do, instead of re-parsing
//! a manifest by hand (which only understands a literal `version = "x.y.z"`
//! string, not workspace inheritance) or guessing a package's directory
//! from an anvil-configured module name (which only works when that name
//! happens to double as a real path prefix, as it does for forge's own
//! `docker/<service>/` layout but not every workspace's).

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct WorkspacePackage {
    pub name: String,
    pub dir: PathBuf,
    pub manifest: PathBuf,
    pub version: String,
    /// `None` = publishable anywhere (the default); `Some(&[])` =
    /// `publish = false`; `Some([registry, ...])` = restricted to those
    /// registries specifically.
    pub publish: Option<Vec<String>>,
}

impl WorkspacePackage {
    /// Whether `cargo publish` would refuse this package outright, per its
    /// own `publish = false`. A `[registry, ...]` restriction list is a
    /// different thing - "publish, but only there" - and is left for
    /// `cargo publish` itself to enforce or reject.
    #[must_use]
    pub const fn publish_disabled(&self) -> bool {
        matches!(&self.publish, Some(registries) if registries.is_empty())
    }

    /// This package's directory, relative to the workspace root - what a
    /// Dockerfile build context or `RESOURCES_PATH` build-arg should name,
    /// however deep or shallow the package actually lives.
    pub fn relative_dir(&self, workspace_root: &Path) -> Result<PathBuf> {
        self.dir
            .strip_prefix(workspace_root)
            .map(Path::to_path_buf)
            .with_context(|| {
                format!(
                    "Package dir {} is not under workspace root {}",
                    self.dir.display(),
                    workspace_root.display()
                )
            })
    }
}

/// Runs `cargo metadata` once.
///
/// Callers needing more than one fact about the workspace (version,
/// publish, directory, dependency graph...) should share a single call
/// rather than re-invoking `cargo metadata` per lookup.
pub fn cargo_metadata() -> Result<Value> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version=1")
        .output()
        .context("Failed to execute cargo metadata")?;

    if !output.status.success() {
        anyhow::bail!("cargo metadata failed");
    }

    serde_json::from_slice(&output.stdout).context("Failed to parse cargo metadata")
}

pub fn workspace_root(metadata: &Value) -> Result<PathBuf> {
    metadata["workspace_root"]
        .as_str()
        .map(PathBuf::from)
        .context("Invalid cargo metadata: missing workspace_root")
}

pub fn workspace_packages(metadata: &Value) -> Result<Vec<WorkspacePackage>> {
    let member_ids = metadata["workspace_members"]
        .as_array()
        .map_or_else(Vec::new, |members| {
            members
                .iter()
                .filter_map(|v| v.as_str())
                .map(ToOwned::to_owned)
                .collect()
        });

    let packages = metadata["packages"]
        .as_array()
        .context("Invalid cargo metadata: missing packages")?;

    Ok(member_ids
        .iter()
        .filter_map(|id| {
            let pkg = packages
                .iter()
                .find(|pkg| pkg["id"].as_str().unwrap_or_default() == id)?;
            let manifest = PathBuf::from(pkg["manifest_path"].as_str()?);
            let dir = manifest.parent()?.to_path_buf();
            let version = pkg["version"].as_str()?.to_string();
            let publish = pkg.get("publish").and_then(Value::as_array).map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.as_str().map(ToOwned::to_owned))
                    .collect()
            });
            Some(WorkspacePackage {
                name: pkg["name"].as_str()?.to_string(),
                dir,
                manifest,
                version,
                publish,
            })
        })
        .collect())
}

/// One package's metadata, resolved from a fresh `cargo metadata` call.
///
/// For callers that only need a single package and would rather not thread
/// `Value`/`Vec<WorkspacePackage>` through their own call chain.
pub fn resolve_package(name: &str) -> Result<WorkspacePackage> {
    let metadata = cargo_metadata()?;
    workspace_packages(&metadata)?
        .into_iter()
        .find(|pkg| pkg.name == name)
        .with_context(|| format!("Package '{name}' not found in workspace metadata"))
}
