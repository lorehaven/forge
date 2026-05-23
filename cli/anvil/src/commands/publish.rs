use crate::config::Config;
use crate::util::run_command;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

pub fn publish(config: &Config, package: Option<String>, all: bool) -> Result<()> {
    if config.publish.registry.trim().is_empty() {
        anyhow::bail!("Missing publish registry. Set [publish].registry in .anvil.toml");
    }

    let metadata = cargo_metadata()?;
    let targets = resolve_publish_targets(config, &metadata, package, all)?;

    for package_name in targets {
        let mut cmd = Command::new("cargo");
        cmd.arg("publish")
            .arg("--registry")
            .arg(&config.publish.registry)
            .arg("--package")
            .arg(&package_name);
        run_command(cmd, &format!("publish ({package_name})"))?;
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

fn workspace_member_names(metadata: &Value) -> Result<Vec<String>> {
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
            packages.iter().find_map(|pkg| {
                if pkg["id"].as_str().unwrap_or_default() == id {
                    pkg["name"].as_str().map(ToOwned::to_owned)
                } else {
                    None
                }
            })
        })
        .collect())
}

fn current_package_name(metadata: &Value) -> Result<Option<String>> {
    let cwd = std::env::current_dir().context("Failed to read current directory")?;
    let cwd_canon = cwd.canonicalize().context("Failed to canonicalize cwd")?;
    let packages = metadata["packages"]
        .as_array()
        .context("Invalid cargo metadata: missing packages")?;

    Ok(packages.iter().find_map(|pkg| {
        let manifest = pkg["manifest_path"].as_str()?;
        let manifest_parent = Path::new(manifest).parent()?;
        let manifest_canon = manifest_parent.canonicalize().ok()?;
        if manifest_canon == cwd_canon {
            pkg["name"].as_str().map(ToOwned::to_owned)
        } else {
            None
        }
    }))
}

fn resolve_single_package(metadata: &Value) -> Result<String> {
    if let Some(pkg_name) = current_package_name(metadata)? {
        return Ok(pkg_name);
    }

    let members = workspace_member_names(metadata)?;
    if members.len() == 1 {
        return Ok(members[0].clone());
    }

    anyhow::bail!(
        "Could not determine publish target. Use --package <name> in workspace roots, or --all"
    )
}

fn ensure_workspace_member(metadata: &Value, package: &str) -> Result<()> {
    let members = workspace_member_names(metadata)?;
    if members.iter().any(|m| m == package) {
        return Ok(());
    }

    anyhow::bail!("Package '{package}' is not a workspace member");
}

fn resolve_all_packages(config: &Config, metadata: &Value) -> Result<Vec<String>> {
    if config.publish.packages.is_empty() {
        anyhow::bail!("No publish packages configured. Set [publish].packages in .anvil.toml");
    }

    for pkg in &config.publish.packages {
        ensure_workspace_member(metadata, pkg)?;
    }

    Ok(config.publish.packages.clone())
}

fn resolve_publish_targets(
    config: &Config,
    metadata: &Value,
    package: Option<String>,
    all: bool,
) -> Result<Vec<String>> {
    if all {
        return resolve_all_packages(config, metadata);
    }

    let package_name = if let Some(pkg) = package {
        ensure_workspace_member(metadata, &pkg)?;
        pkg
    } else {
        resolve_single_package(metadata)?
    };

    Ok(vec![package_name])
}
