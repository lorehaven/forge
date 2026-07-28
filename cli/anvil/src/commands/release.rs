use crate::commands::{docker, install};
use crate::config::Config;
use crate::util::run_command;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::{DocumentMut, value};

#[derive(Debug)]
struct WorkspacePackage {
    name: String,
    dir: PathBuf,
    manifest: PathBuf,
}

#[derive(Debug, Clone, Copy)]
enum ReleaseKind {
    Docker,
    Cargo,
}

#[derive(Debug, Clone)]
struct ReleasePlanItem {
    package: String,
    from_version: String,
    to_version: String,
    kind: ReleaseKind,
    tag_to_create: String,
    bump_version: bool,
    install_after_publish: bool,
    layer: usize,
}

pub fn release(config: &Config, package: Option<String>, all: bool, dry_run: bool) -> Result<()> {
    let metadata = cargo_metadata()?;
    let targets = resolve_release_targets(config, &metadata, package, all)?;
    let mut plan = build_release_plan(config, &metadata, &targets)?;

    ensure_release_plan_non_empty(all, &plan, dry_run)?;
    if plan.is_empty() {
        return Ok(());
    }

    // Sort plan by layer for execution order
    plan.sort_by_key(|item| item.layer);

    // Always show the dry-run first
    print_dry_run_plan_with_layers(&plan);

    if dry_run {
        return Ok(());
    }

    // Ask for confirmation before proceeding
    if !confirm_release()? {
        println!("Release cancelled.");
        return Ok(());
    }

    ensure_release_tags_absent(&plan)?;
    let manifests = bump_patch_versions(&metadata, &plan)?;
    if !manifests.is_empty() {
        run_build_for_bumped_packages(&plan)?;
        create_version_commit(&metadata, &manifests, &plan)?;
        push_version_commit()?;
    }

    // Release layer by layer
    let max_layer = plan.iter().map(|item| item.layer).max().unwrap_or(0);
    for current_layer in 0..=max_layer {
        let layer_items: Vec<_> = plan
            .iter()
            .filter(|item| item.layer == current_layer)
            .collect();
        if layer_items.is_empty() {
            continue;
        }

        println!("\nReleasing Layer {current_layer}...");
        for item in layer_items {
            match item.kind {
                ReleaseKind::Docker => {
                    docker::release(config, &item.package)?;
                }
                ReleaseKind::Cargo => {
                    let mut cmd = Command::new("cargo");
                    cmd.arg("publish")
                        .arg("--registry")
                        .arg(&config.release.registry)
                        .arg("--package")
                        .arg(&item.package);
                    run_command(cmd, &format!("publish ({})", item.package))?;
                    if item.install_after_publish {
                        install::install(config, Some(item.package.clone()), false)?;
                    }
                }
            }

            // Tagged here rather than once at the end, because a publish cannot
            // be undone. If a later package fails, everything already released
            // keeps its tag, and the repository still says what shipped - where
            // tagging at the end would leave those versions in the registry
            // with nothing in git to name them.
            create_release_tag(item)?;
        }
    }

    println!("\n✅ Release completed successfully");
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

fn workspace_packages(metadata: &Value) -> Result<Vec<WorkspacePackage>> {
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
            Some(WorkspacePackage {
                name: pkg["name"].as_str()?.to_string(),
                dir,
                manifest,
            })
        })
        .collect())
}

fn resolve_release_targets(
    config: &Config,
    metadata: &Value,
    package: Option<String>,
    all: bool,
) -> Result<Vec<String>> {
    let members: HashSet<String> = workspace_packages(metadata)?
        .into_iter()
        .map(|pkg| pkg.name)
        .collect();

    if all {
        let mut targets = Vec::new();
        for pkg in &config.release.packages {
            targets.push(pkg.clone());
        }
        for module in config.docker.modules.values() {
            for pkg in &module.packages {
                if !targets.contains(pkg) {
                    targets.push(pkg.clone());
                }
            }
        }

        if targets.is_empty() {
            anyhow::bail!(
                "No release packages configured. Set [release].packages and/or [docker.modules.*].packages in .anvil.toml"
            );
        }

        for pkg in &targets {
            if !members.contains(pkg) {
                anyhow::bail!("Configured release package '{pkg}' is not a workspace member");
            }
        }

        return Ok(targets);
    }

    let package_name = if let Some(pkg) = package {
        if !members.contains(&pkg) {
            anyhow::bail!("Package '{pkg}' is not a workspace member");
        }
        pkg
    } else {
        resolve_single_package(metadata)?
    };

    Ok(vec![package_name])
}

fn resolve_single_package(metadata: &Value) -> Result<String> {
    let cwd = std::env::current_dir().context("Failed to read current directory")?;
    let cwd_canon = cwd.canonicalize().context("Failed to canonicalize cwd")?;
    let packages = workspace_packages(metadata)?;

    if let Some(pkg) = packages.iter().find(|pkg| {
        pkg.dir
            .canonicalize()
            .is_ok_and(|manifest_canon| manifest_canon == cwd_canon)
    }) {
        return Ok(pkg.name.clone());
    }

    if packages.len() == 1 {
        return Ok(packages[0].name.clone());
    }

    anyhow::bail!("Could not determine release target. Use --package <name> or --all")
}

#[allow(clippy::too_many_lines)]
fn build_release_plan(
    config: &Config,
    metadata: &Value,
    targets: &[String],
) -> Result<Vec<ReleasePlanItem>> {
    let workspace = workspace_packages(metadata)?;
    let workspace_root = PathBuf::from(
        metadata["workspace_root"]
            .as_str()
            .context("Invalid cargo metadata: missing workspace_root")?,
    );
    let dep_graph = build_dependency_graph(metadata)?;

    // Include transitive dependencies of targets
    let mut all_targets: HashSet<String> = targets.iter().cloned().collect();
    for target in targets {
        let mut visited = HashSet::new();
        get_transitive_dependencies(target, &dep_graph, &mut visited);
        all_targets.extend(visited);
    }
    let all_targets: Vec<String> = all_targets.into_iter().collect();

    // Identify which packages actually need release (have changes or dependencies changed)
    let mut packages_to_release: HashSet<String> = HashSet::new();
    let mut packages_with_changes: HashSet<String> = HashSet::new();

    for package_name in &all_targets {
        let pkg = workspace
            .iter()
            .find(|pkg| &pkg.name == package_name)
            .with_context(|| format!("Package '{package_name}' not found in workspace metadata"))?;

        if let Some(last_tag) = latest_package_tag(package_name)? {
            if package_changed_since_tag(&workspace_root, &pkg.dir, &last_tag)? {
                packages_to_release.insert(package_name.clone());
                packages_with_changes.insert(package_name.clone());
            }
        } else {
            // First release
            packages_to_release.insert(package_name.clone());
            packages_with_changes.insert(package_name.clone());
        }
    }

    // Auto-bump packages that depend on modified packages
    loop {
        let mut changed = false;
        for package_name in &all_targets {
            if packages_to_release.contains(package_name) {
                continue;
            }
            if let Some(deps) = dep_graph.get(package_name)
                && deps.iter().any(|dep| packages_with_changes.contains(dep))
            {
                packages_to_release.insert(package_name.clone());
                packages_with_changes.insert(package_name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Compute layers for all packages to release
    let layers = compute_package_layers(
        &packages_to_release.iter().cloned().collect::<Vec<_>>(),
        &dep_graph,
    );

    // Build plan items
    let mut plan = Vec::new();
    for package_name in &packages_to_release {
        let pkg = workspace
            .iter()
            .find(|pkg| &pkg.name == package_name)
            .with_context(|| format!("Package '{package_name}' not found in workspace metadata"))?;
        let current_version = read_manifest_version(&pkg.manifest)?;
        let kind = if is_docker_package(config, package_name) {
            ReleaseKind::Docker
        } else {
            ReleaseKind::Cargo
        };
        let install_after_publish = should_install_package(config, package_name);
        let current_version_tag = package_tag_name(package_name, &current_version);
        let current_version_tag_exists = tag_exists(&current_version_tag)?;
        let layer = layers.get(package_name).copied().unwrap_or(0);

        if let Some(_last_tag) = latest_package_tag(package_name)? {
            if current_version_tag_exists {
                let next_version = bump_patch(&current_version)?;
                plan.push(ReleasePlanItem {
                    package: package_name.clone(),
                    from_version: current_version,
                    to_version: next_version.clone(),
                    kind,
                    tag_to_create: package_tag_name(package_name, &next_version),
                    bump_version: true,
                    install_after_publish,
                    layer,
                });
            } else {
                plan.push(ReleasePlanItem {
                    package: package_name.clone(),
                    from_version: current_version.clone(),
                    to_version: current_version.clone(),
                    kind,
                    tag_to_create: current_version_tag,
                    bump_version: false,
                    install_after_publish,
                    layer,
                });
            }
        } else {
            plan.push(ReleasePlanItem {
                package: package_name.clone(),
                from_version: current_version.clone(),
                to_version: current_version.clone(),
                kind,
                tag_to_create: package_tag_name(package_name, &current_version),
                bump_version: false,
                install_after_publish,
                layer,
            });
        }
    }

    Ok(plan)
}

fn latest_package_tag(package: &str) -> Result<Option<String>> {
    let pattern = format!("{package}-v*");
    let output = Command::new("git")
        .args(["tag", "--list", &pattern, "--sort=-v:refname"])
        .output()
        .with_context(|| format!("Failed to execute git tag --list {pattern}"))?;
    if !output.status.success() {
        anyhow::bail!("git tag --list {pattern} failed");
    }

    let tag = String::from_utf8(output.stdout)
        .context("Invalid UTF-8 in git tag output")?
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned);
    Ok(tag)
}

fn package_changed_since_tag(workspace_root: &Path, package_dir: &Path, tag: &str) -> Result<bool> {
    let relative_dir = package_dir.strip_prefix(workspace_root).with_context(|| {
        format!(
            "Package dir {} is not under workspace root {}",
            package_dir.display(),
            workspace_root.display()
        )
    })?;
    let range = format!("{tag}..HEAD");
    let pathspec = relative_dir
        .to_str()
        .context("Package path is not valid UTF-8 for git pathspec")?;

    let output = Command::new("git")
        .args(["diff", "--name-only", &range, "--", pathspec])
        .output()
        .with_context(|| format!("Failed to execute git diff --name-only {range} -- {pathspec}"))?;

    if !output.status.success() {
        anyhow::bail!("git diff --name-only {range} -- {pathspec} failed");
    }

    let has_changes = !String::from_utf8(output.stdout)
        .context("Invalid UTF-8 in git diff output")?
        .trim()
        .is_empty();
    Ok(has_changes)
}

fn ensure_release_plan_non_empty(all: bool, plan: &[ReleasePlanItem], dry_run: bool) -> Result<()> {
    if !plan.is_empty() {
        return Ok(());
    }

    if dry_run {
        println!("No packages need release.");
        return Ok(());
    }

    if all {
        anyhow::bail!("No packages require release from [release].packages");
    }
    anyhow::bail!("Target package has no changes since its last tag");
}

fn read_manifest_version(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest at {}", path.display()))?;
    let value: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Failed to parse manifest at {}", path.display()))?;
    value
        .get("package")
        .and_then(toml::Value::as_table)
        .and_then(|table| table.get("version"))
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned)
        .context("Manifest missing package.version")
}

const fn release_action_label(item: &ReleasePlanItem) -> &'static str {
    match item.kind {
        ReleaseKind::Docker => "docker release",
        ReleaseKind::Cargo if item.install_after_publish => "cargo publish + install",
        ReleaseKind::Cargo => "cargo publish",
    }
}

fn bump_patch_versions(metadata: &Value, plan: &[ReleasePlanItem]) -> Result<Vec<PathBuf>> {
    let workspace = workspace_packages(metadata)?;
    let mut manifests = Vec::new();

    for item in plan {
        if !item.bump_version {
            continue;
        }

        let pkg = workspace
            .iter()
            .find(|pkg| pkg.name.as_str() == item.package.as_str())
            .with_context(|| {
                format!("Package '{}' not found in workspace metadata", item.package)
            })?;

        set_manifest_version(&pkg.manifest, &item.to_version)?;
        manifests.push(pkg.manifest.clone());
    }

    Ok(manifests)
}

fn set_manifest_version(path: &Path, version: &str) -> Result<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest at {}", path.display()))?;

    let mut doc = content
        .parse::<DocumentMut>()
        .with_context(|| format!("Failed to parse manifest at {}", path.display()))?;

    doc["package"]["version"] = value(version);

    fs::write(path, doc.to_string())
        .with_context(|| format!("Failed to write manifest at {}", path.display()))?;

    Ok(())
}

fn bump_patch(version: &str) -> Result<String> {
    let mut parts = version.split('.');
    let major = parts
        .next()
        .context("Invalid version: missing major")?
        .parse::<u64>()
        .context("Invalid version major")?;
    let minor = parts
        .next()
        .context("Invalid version: missing minor")?
        .parse::<u64>()
        .context("Invalid version minor")?;
    let patch = parts
        .next()
        .context("Invalid version: missing patch")?
        .parse::<u64>()
        .context("Invalid version patch")?;

    if parts.next().is_some() {
        anyhow::bail!("Invalid version format '{version}', expected MAJOR.MINOR.PATCH");
    }

    Ok(format!("{major}.{minor}.{}", patch + 1))
}

fn run_build_for_bumped_packages(plan: &[ReleasePlanItem]) -> Result<()> {
    for item in plan {
        if !item.bump_version {
            continue;
        }
        let mut cmd = Command::new("cargo");
        cmd.arg("build").arg("--package").arg(&item.package);
        run_command(cmd, &format!("build ({})", item.package))?;
    }
    Ok(())
}

fn create_version_commit(
    metadata: &Value,
    manifests: &[PathBuf],
    plan: &[ReleasePlanItem],
) -> Result<()> {
    let mut commit_paths = manifests.to_vec();
    let workspace_root = PathBuf::from(
        metadata["workspace_root"]
            .as_str()
            .context("Invalid cargo metadata: missing workspace_root")?,
    );
    let cargo_lock = workspace_root.join("Cargo.lock");
    if cargo_lock.exists() {
        commit_paths.push(cargo_lock);
    }

    let mut add_cmd = Command::new("git");
    add_cmd.arg("add").args(&commit_paths);
    run_command(add_cmd, "git add version update files")?;

    let package_summaries = plan
        .iter()
        .filter(|item| item.bump_version)
        .map(|item| format!("{} v{}", item.package, item.to_version))
        .collect::<Vec<_>>()
        .join(", ");

    let commit_message = format!("release: bump package versions ({package_summaries})");

    let mut commit_cmd = Command::new("git");
    commit_cmd.arg("commit").arg("-m").arg(commit_message);

    commit_cmd.arg("--");
    commit_cmd.args(&commit_paths);
    run_command(commit_cmd, "git commit version update")?;
    Ok(())
}

fn push_version_commit() -> Result<()> {
    let mut push_cmd = Command::new("git");
    push_cmd.arg("push");
    run_command(push_cmd, "git push version update commit")
}

fn package_tag_name(package: &str, version: &str) -> String {
    format!("{package}-v{version}")
}

fn tag_exists(tag: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["tag", "-l", tag])
        .output()
        .with_context(|| format!("Failed to execute git tag -l {tag}"))?;
    if !output.status.success() {
        anyhow::bail!("git tag -l {tag} failed");
    }
    let found = String::from_utf8(output.stdout)
        .context("Invalid UTF-8 in git tag output")?
        .lines()
        .any(|line| line.trim() == tag);
    Ok(found)
}

fn ensure_release_tags_absent(plan: &[ReleasePlanItem]) -> Result<()> {
    for item in plan {
        if tag_exists(&item.tag_to_create)? {
            anyhow::bail!("Tag '{}' already exists", item.tag_to_create);
        }
    }
    Ok(())
}

/// Tags one package, immediately after it has been released.
fn create_release_tag(item: &ReleasePlanItem) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("tag").arg(&item.tag_to_create);
    run_command(cmd, &format!("git tag {}", item.tag_to_create))
}

fn is_docker_package(config: &Config, package: &str) -> bool {
    config
        .docker
        .modules
        .values()
        .any(|module| module.packages.iter().any(|p| p == package))
}

fn should_install_package(config: &Config, package: &str) -> bool {
    config.install.packages.iter().any(|p| p == package)
}

fn build_dependency_graph(metadata: &Value) -> Result<HashMap<String, Vec<String>>> {
    let packages = workspace_packages(metadata)?;
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    for pkg in &packages {
        let manifest_content = fs::read_to_string(&pkg.manifest)
            .with_context(|| format!("Failed to read manifest at {}", pkg.manifest.display()))?;
        let manifest_value: toml::Value = toml::from_str(&manifest_content)
            .with_context(|| format!("Failed to parse manifest at {}", pkg.manifest.display()))?;

        let mut deps = Vec::new();
        if let Some(dependencies) = manifest_value
            .get("dependencies")
            .and_then(|d| d.as_table())
        {
            for dep_name in dependencies.keys() {
                if packages.iter().any(|p| &p.name == dep_name) {
                    deps.push(dep_name.clone());
                }
            }
        }
        if let Some(dev_dependencies) = manifest_value
            .get("dev-dependencies")
            .and_then(|d| d.as_table())
        {
            for dep_name in dev_dependencies.keys() {
                if packages.iter().any(|p| &p.name == dep_name) && !deps.contains(dep_name) {
                    deps.push(dep_name.clone());
                }
            }
        }
        graph.insert(pkg.name.clone(), deps);
    }

    Ok(graph)
}

fn get_transitive_dependencies(
    package: &str,
    graph: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
) {
    if visited.contains(package) {
        return;
    }
    visited.insert(package.to_string());

    if let Some(deps) = graph.get(package) {
        for dep in deps {
            get_transitive_dependencies(dep, graph, visited);
        }
    }
}

fn compute_package_layers(
    packages: &[String],
    graph: &HashMap<String, Vec<String>>,
) -> HashMap<String, usize> {
    let packages_set: HashSet<&String> = packages.iter().collect();
    let mut layers: HashMap<String, usize> = HashMap::new();

    for pkg in packages {
        let mut layer = 0;
        if let Some(deps) = graph.get(pkg) {
            for dep in deps {
                if packages_set.contains(dep) {
                    layer = layer.max(layers.get(dep).copied().unwrap_or(0) + 1);
                }
            }
        }
        layers.insert(pkg.clone(), layer);
    }

    // Iteratively refine layers until stable
    for _ in 0..packages.len() {
        let mut changed = false;
        for pkg in packages {
            let mut new_layer = 0;
            if let Some(deps) = graph.get(pkg) {
                for dep in deps {
                    if packages_set.contains(dep) {
                        new_layer = new_layer.max(layers.get(dep).copied().unwrap_or(0) + 1);
                    }
                }
            }
            if new_layer > layers[pkg] {
                layers.insert(pkg.clone(), new_layer);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    layers
}

fn print_dry_run_plan_with_layers(plan: &[ReleasePlanItem]) {
    println!("Dry run: planned releases (organized by dependency layers)\n");

    let max_layer = plan.iter().map(|item| item.layer).max().unwrap_or(0);

    for current_layer in 0..=max_layer {
        let layer_items: Vec<_> = plan
            .iter()
            .filter(|item| item.layer == current_layer)
            .collect();
        if layer_items.is_empty() {
            continue;
        }

        println!("Layer {current_layer}:");
        for item in layer_items {
            let version_note = if item.bump_version {
                format!("{} -> {}", item.from_version, item.to_version)
            } else {
                format!("{} (no bump; initial package tag)", item.from_version)
            };
            let action = release_action_label(item);
            println!(
                "  - {}: {} ({}), tag: {}",
                item.package, version_note, action, item.tag_to_create
            );
        }
        println!();
    }
}

fn confirm_release() -> Result<bool> {
    print!("Proceed with release? (yes/no): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().eq_ignore_ascii_case("yes") || input.trim().eq_ignore_ascii_case("y"))
}
