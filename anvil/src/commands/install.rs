use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::run_command;

pub fn install(package: Option<String>, all: bool) -> Result<()> {
    let metadata = cargo_metadata()?;
    let targets = resolve_install_targets(&metadata, package, all)?;
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

fn resolve_install_package(metadata: &Value, package: Option<String>) -> Result<String> {
    if let Some(pkg) = package {
        return Ok(pkg);
    }

    let members = metadata["workspace_members"].as_array().map_or(0, Vec::len);

    if members > 1 {
        anyhow::bail!(
            "Multiple workspace packages detected. Use --package <name> with `anvil install`."
        );
    }

    default_module_name(metadata)
}

fn resolve_workspace_member_ids(metadata: &Value) -> Vec<String> {
    metadata["workspace_members"]
        .as_array()
        .map_or_else(Vec::new, |members| {
            members
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
}

fn resolve_workspace_install_targets(metadata: &Value) -> Result<Vec<(String, PathBuf)>> {
    let member_ids = resolve_workspace_member_ids(metadata);
    let packages = metadata["packages"]
        .as_array()
        .context("Invalid cargo metadata: missing packages")?;

    let mut out = Vec::new();
    for id in member_ids {
        let maybe = packages.iter().find_map(|pkg| {
            let pkg_id = pkg["id"].as_str()?;
            if pkg_id != id {
                return None;
            }
            let name = pkg["name"].as_str()?.to_string();
            let manifest = pkg["manifest_path"].as_str()?;
            let path = Path::new(manifest).parent()?.to_path_buf();
            Some((name, path))
        });
        if let Some(item) = maybe {
            out.push(item);
        }
    }

    if out.is_empty() {
        anyhow::bail!("No workspace packages found to install");
    }
    Ok(out)
}

fn resolve_install_targets(
    metadata: &Value,
    package: Option<String>,
    all: bool,
) -> Result<Vec<(String, PathBuf)>> {
    if all {
        return resolve_workspace_install_targets(metadata);
    }

    let package_name = resolve_install_package(metadata, package)?;
    let package_path = resolve_package_path(metadata, &package_name)?;
    Ok(vec![(package_name, package_path)])
}

fn resolve_package_path(metadata: &Value, package_name: &str) -> Result<PathBuf> {
    let packages = metadata["packages"]
        .as_array()
        .context("Invalid cargo metadata: missing packages")?;

    let maybe_path = packages.iter().find_map(|pkg| {
        let name = pkg["name"].as_str()?;
        if name != package_name {
            return None;
        }
        let manifest = pkg["manifest_path"].as_str()?;
        Path::new(manifest).parent().map(Path::to_path_buf)
    });

    maybe_path.with_context(|| format!("Package '{package_name}' not found in cargo metadata"))
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
    let single_member = packages.iter().find_map(|pkg| {
        let manifest = pkg["manifest_path"].as_str()?;
        let manifest_parent = Path::new(manifest).parent()?.to_path_buf();
        if let Some(root) = &workspace_root
            && manifest_parent.starts_with(root)
        {
            return pkg["name"].as_str().map(ToOwned::to_owned);
        }
        None
    });

    single_member.context("Could not determine default package name. Use --package <name>.")
}

#[cfg(test)]
mod tests {
    use super::{resolve_install_package, resolve_install_targets, resolve_package_path};
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn install_requires_package_for_multi_member_workspace() {
        let metadata = json!({
            "workspace_members": ["a 0.1.0 (path+file:///x/a)", "b 0.1.0 (path+file:///x/b)"],
            "packages": []
        });
        assert!(resolve_install_package(&metadata, None).is_err());
    }

    #[test]
    fn install_uses_explicit_package_when_provided() {
        let metadata = json!({
            "workspace_members": ["a 0.1.0 (path+file:///x/a)", "b 0.1.0 (path+file:///x/b)"],
            "packages": []
        });
        let pkg = resolve_install_package(&metadata, Some("ferrous".to_string())).unwrap();
        assert_eq!(pkg, "ferrous");
    }

    #[test]
    fn resolve_package_path_uses_manifest_parent() {
        let metadata = json!({
            "packages": [
                { "name": "anvil", "manifest_path": "/repo/anvil/Cargo.toml" }
            ]
        });
        let path = resolve_package_path(&metadata, "anvil").unwrap();
        assert_eq!(path, PathBuf::from("/repo/anvil"));
    }

    #[test]
    fn resolve_install_targets_all_uses_workspace_members() {
        let metadata = json!({
            "workspace_members": ["anvil 0.1.0 (path+file:///repo/anvil)", "ferrous 0.1.0 (path+file:///repo/ferrous)"],
            "packages": [
                {
                    "id": "anvil 0.1.0 (path+file:///repo/anvil)",
                    "name": "anvil",
                    "manifest_path": "/repo/anvil/Cargo.toml"
                },
                {
                    "id": "ferrous 0.1.0 (path+file:///repo/ferrous)",
                    "name": "ferrous",
                    "manifest_path": "/repo/ferrous/Cargo.toml"
                }
            ]
        });
        let targets = resolve_install_targets(&metadata, None, true).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].0, "anvil");
        assert_eq!(targets[1].0, "ferrous");
        assert_eq!(targets[0].1, PathBuf::from("/repo/anvil"));
        assert_eq!(targets[1].1, PathBuf::from("/repo/ferrous"));
    }
}
