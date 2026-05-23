use crate::config::Config;
use crate::util::run_command;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn install(config: &Config, package: Option<String>, all: bool) -> Result<()> {
    let metadata = cargo_metadata()?;
    let targets = resolve_install_targets(config, &metadata, package, all)?;

    for (name, path) in targets {
        let mut cmd = Command::new("cargo");
        cmd.arg("install").arg("--path").arg(&path);
        run_command(cmd, &format!("install ({name})"))?;
    }

    Ok(())
}

fn cargo_metadata() -> Result<Value> {
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

fn resolve_workspace_install_targets(
    config: &Config,
    metadata: &Value,
) -> Result<Vec<(String, PathBuf)>> {
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

    let mut out = Vec::new();
    for id in member_ids {
        if let Some(pkg) = packages
            .iter()
            .find(|pkg| pkg["id"].as_str().unwrap_or_default() == id)
        {
            let name = pkg["name"].as_str().unwrap_or_default().to_string();

            // Filter by config.install.packages if present
            if !config.install.packages.is_empty() && !config.install.packages.contains(&name) {
                continue;
            }

            let manifest = pkg["manifest_path"].as_str().unwrap_or_default();
            let path = Path::new(manifest).parent().unwrap().to_path_buf();
            out.push((name, path));
        }
    }

    if out.is_empty() {
        anyhow::bail!("No workspace packages found to install (after filtering by config)");
    }

    Ok(out)
}

fn resolve_install_targets(
    config: &Config,
    metadata: &Value,
    package: Option<String>,
    all: bool,
) -> Result<Vec<(String, PathBuf)>> {
    if all {
        return resolve_workspace_install_targets(config, metadata);
    }

    // Check config install packages first
    if let Some(pkg_name) = package.or_else(|| config.install.packages.first().cloned()) {
        let path = resolve_package_path(metadata, &pkg_name)?;
        return Ok(vec![(pkg_name, path)]);
    }

    // Fallback: determine default workspace package
    let pkg_name = default_module_name(metadata)?;
    let path = resolve_package_path(metadata, &pkg_name)?;
    Ok(vec![(pkg_name, path)])
}

fn resolve_package_path(metadata: &Value, package_name: &str) -> Result<PathBuf> {
    let packages = metadata["packages"]
        .as_array()
        .context("Invalid cargo metadata: missing packages")?;

    packages
        .iter()
        .find_map(|pkg| {
            let name = pkg["name"].as_str()?;
            if name != package_name {
                return None;
            }
            let manifest = pkg["manifest_path"].as_str()?;
            Path::new(manifest).parent().map(Path::to_path_buf)
        })
        .with_context(|| format!("Package '{package_name}' not found in cargo metadata"))
}

fn default_module_name(metadata: &Value) -> Result<String> {
    let cwd = std::env::current_dir().context("Failed to read current directory")?;
    let packages = metadata["packages"]
        .as_array()
        .context("Invalid cargo metadata: missing packages")?;

    if let Some(pkg_name) = packages.iter().find_map(|pkg| {
        let manifest = pkg["manifest_path"].as_str()?;
        let manifest_parent = Path::new(manifest).parent()?;
        let manifest_canon = manifest_parent.canonicalize().ok()?;
        let cwd_canon = cwd.canonicalize().ok()?;
        if manifest_canon == cwd_canon {
            pkg["name"].as_str().map(ToOwned::to_owned)
        } else {
            None
        }
    }) {
        return Ok(pkg_name);
    }

    // Fallback: when invoked from workspace root with a single member, use that member.
    let workspace_root = metadata["workspace_root"].as_str().map(PathBuf::from);
    packages
        .iter()
        .find_map(|pkg| {
            let manifest = pkg["manifest_path"].as_str()?;
            let manifest_parent = Path::new(manifest).parent()?.to_path_buf();
            if let Some(root) = &workspace_root
                && manifest_parent.starts_with(root)
            {
                return pkg["name"].as_str().map(ToOwned::to_owned);
            }
            None
        })
        .context("Could not determine default package name. Use --package <name>.")
}
