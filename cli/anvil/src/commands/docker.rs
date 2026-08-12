use crate::cargo_meta;
use crate::config;
use crate::util::run_command;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

fn find_module_for_package<'a>(config: &'a config::Config, package: &str) -> Result<&'a str> {
    for (module, module_cfg) in &config.docker.modules {
        if module_cfg.packages.iter().any(|p| p == package) {
            return Ok(module);
        }
    }

    anyhow::bail!("Module not found for package: {package}")
}

fn find_module_name_overwrite(config: &config::Config, package: &str) -> Result<String> {
    let module = find_module_for_package(config, package)?;
    let module_cfg = &config.docker.modules[module];

    Ok(module_cfg
        .package_overrides
        .get(package)
        .and_then(|override_cfg| override_cfg.module_name.as_ref())
        .map_or_else(|| module.to_string(), Clone::clone))
}

fn get_dockerfile_for_package(config: &config::Config, package: &str) -> Result<String> {
    let module = find_module_for_package(config, package)?;
    let module_cfg = &config.docker.modules[module];

    Ok(module_cfg
        .package_overrides
        .get(package)
        .and_then(|override_cfg| override_cfg.dockerfile.as_ref())
        .map_or_else(|| module_cfg.dockerfile.clone(), Clone::clone))
}

/// The package's own `--build-arg` values, if it declared any.
///
/// Empty for most packages: the Dockerfile's defaults describe a plain web
/// service, and only a package that is not one has anything to say here.
fn get_build_args_for_package(
    config: &config::Config,
    package: &str,
) -> Result<BTreeMap<String, String>> {
    let module = find_module_for_package(config, package)?;
    let module_cfg = &config.docker.modules[module];

    Ok(module_cfg
        .package_overrides
        .get(package)
        .map(|override_cfg| override_cfg.build_args.clone())
        .unwrap_or_default())
}

fn get_image_name_for_package(config: &config::Config, package: &str) -> Result<String> {
    let module = find_module_for_package(config, package)?;
    let module_cfg = &config.docker.modules[module];

    Ok(module_cfg
        .package_overrides
        .get(package)
        .and_then(|override_cfg| override_cfg.image_name.as_ref())
        .map_or_else(|| package.to_string(), Clone::clone))
}

fn get_registries_for_package(config: &config::Config, package: &str) -> Result<Vec<String>> {
    let module = find_module_for_package(config, package)?;
    let module_cfg = &config.docker.modules[module];

    let registries = module_cfg.package_overrides.get(package).map_or_else(
        || {
            if config.docker.registry.trim().is_empty() {
                Vec::new()
            } else {
                vec![config.docker.registry.clone()]
            }
        },
        |override_cfg| {
            if !override_cfg.registries.is_empty() {
                override_cfg.registries.clone()
            } else if let Some(registry) = override_cfg.registry.as_ref() {
                vec![registry.clone()]
            } else if !config.docker.registry.trim().is_empty() {
                vec![config.docker.registry.clone()]
            } else {
                Vec::new()
            }
        },
    );

    if registries.is_empty() {
        anyhow::bail!(
            "No Docker registry configured for package '{package}'. Set [docker].registry, \
             [docker.modules.<module>.{package}].registries, or \
             [docker.modules.<module>.{package}].registry"
        );
    }

    let filtered: Vec<String> = registries
        .into_iter()
        .filter(|reg| !reg.trim().is_empty())
        .collect();
    if filtered.is_empty() {
        anyhow::bail!("Docker registries for package '{package}' are empty");
    }

    Ok(filtered)
}

/// Cargo's own config search root - `CARGO_HOME` if set, otherwise `~/.cargo`
/// (`HOME` on Unix, `USERPROFILE` on Windows). Same resolution real `cargo`
/// invocations already use.
fn cargo_home() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CARGO_HOME") {
        return Some(PathBuf::from(dir));
    }
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(|home| PathBuf::from(home).join(".cargo"))
}

/// The `[registries.<name>]` table from the host's own `~/.cargo/config.toml`,
/// if present - the same file real `cargo` commands already resolve
/// index/token from, so this is a fallback of last resort, not a guess.
fn private_registry_config(name: &str) -> Option<toml::Value> {
    let path = cargo_home()?.join("config.toml");
    let content = std::fs::read_to_string(path).ok()?;
    let parsed: toml::Value = toml::from_str(&content).ok()?;
    Some(parsed.get("registries")?.get(name)?.clone())
}

/// `cargo`'s own name -> env-var mangling: hyphens become underscores, the
/// name is upper-cased. See `CARGO_REGISTRIES_<NAME>_INDEX`/`_TOKEN` in the
/// cargo reference.
fn env_var_name_for_registry(name: &str, suffix: &str) -> String {
    format!(
        "CARGO_REGISTRIES_{}_{suffix}",
        name.to_uppercase().replace('-', "_")
    )
}

/// The private cargo registry's index URL, if `[docker].cargo_registry` names
/// one in `.anvil.toml`. Resolution order: the matching env var (an explicit
/// override, e.g. from CI) > `.anvil.toml`'s own `cargo_registry_index` > the
/// host's `~/.cargo/config.toml`. Anvil itself has no opinion on what
/// registry a workspace uses or where it lives - that's entirely the
/// workspace's own `.anvil.toml` and cargo config to say.
fn cargo_registry_index(config: &config::Config, name: &str) -> Option<String> {
    if let Ok(index) = std::env::var(env_var_name_for_registry(name, "INDEX")) {
        return Some(index);
    }
    if let Some(index) = config.docker.cargo_registry_index.as_ref().filter(|s| !s.trim().is_empty()) {
        return Some(index.clone());
    }
    private_registry_config(name)?
        .get("index")?
        .as_str()
        .map(str::to_string)
}

fn cargo_registry_token(name: &str) -> Option<String> {
    if let Ok(token) = std::env::var(env_var_name_for_registry(name, "TOKEN"))
        && !token.trim().is_empty()
    {
        return Some(token);
    }
    private_registry_config(name)?
        .get("token")?
        .as_str()
        .map(str::to_string)
}

fn full_tags_for_package(config: &config::Config, package: &str) -> Result<Vec<String>> {
    let registries = get_registries_for_package(config, package)?;
    let module_name = find_module_name_overwrite(config, package)?;
    let image_name = get_image_name_for_package(config, package)?;
    let version = cargo_meta::resolve_package(package)?.version;

    Ok(registries
        .iter()
        .map(|registry| format!("{registry}/{module_name}/{image_name}:{version}"))
        .collect())
}

pub fn build(config: &config::Config, package: &str) -> Result<()> {
    let dockerfile = get_dockerfile_for_package(config, package)?;
    let image_name = get_image_name_for_package(config, package)?;
    let build_args = get_build_args_for_package(config, package)?;

    // The package's own directory, relative to the workspace root - not an
    // anvil module name assumed to double as a path prefix, so this works
    // regardless of whether the workspace nests packages under a module
    // directory (forge's own docker/<service>/ convention) or not.
    let metadata = cargo_meta::cargo_metadata()?;
    let workspace_root = cargo_meta::workspace_root(&metadata)?;
    let pkg = cargo_meta::workspace_packages(&metadata)?
        .into_iter()
        .find(|pkg| pkg.name == package)
        .with_context(|| format!("Package '{package}' not found in workspace metadata"))?;
    let resources_path = pkg.relative_dir(&workspace_root)?;
    let resources_path = resources_path
        .to_str()
        .with_context(|| format!("Package '{package}' path is not valid UTF-8"))?;
    println!("Building Docker image for package: {package} using {dockerfile}");
    if !build_args.is_empty() {
        // Worth printing: these are the whole reason two packages that build
        // from the same file end up as different images.
        let rendered = build_args
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("  build args: {rendered}");
    }

    let mut cmd = Command::new("docker");
    cmd.arg("build")
        .arg("-f")
        .arg(&dockerfile)
        // Some Docker daemons (rootless setups, sandboxed CI hosts) cannot
        // create a veth pair for the default bridge network a build step's
        // container would otherwise get - "operation not supported" from
        // dockerd, before a single instruction runs. `host` needs no new
        // network namespace, so it sidesteps that entirely; a build has no
        // business publishing ports or isolating its network from the host
        // it's running on anyway.
        .arg("--network=host")
        // BuildKit cache mount support
        .arg("--progress=plain")
        .arg("--build-arg")
        .arg("BUILDKIT_INLINE_CACHE=1")
        .arg("--build-arg")
        .arg(format!("PROJECT_NAME={package}"))
        .arg("--build-arg")
        .arg(format!("RESOURCES_PATH={resources_path}"));

    // A private cargo registry the Dockerfile needs to see is entirely a
    // workspace concern - anvil only forwards it if `.anvil.toml` names one.
    if let Some(registry_name) = config
        .docker
        .cargo_registry
        .as_ref()
        .filter(|s| !s.trim().is_empty())
    {
        if let Some(index) = cargo_registry_index(config, registry_name) {
            cmd.arg("--build-arg").arg(format!(
                "{}={index}",
                env_var_name_for_registry(registry_name, "INDEX")
            ));
        }
        if let Some(token) = cargo_registry_token(registry_name) {
            cmd.arg("--build-arg").arg(format!(
                "{}={token}",
                env_var_name_for_registry(registry_name, "TOKEN")
            ));
        }
    }

    // After the ones anvil derives, so a package that genuinely needs a
    // different `RESOURCES_PATH` can say so - docker takes the last value for a
    // repeated argument.
    for (name, value) in &build_args {
        cmd.arg("--build-arg").arg(format!("{name}={value}"));
    }

    cmd.arg("-t").arg(image_name).arg(".");

    // Enable BuildKit with all caching optimizations
    cmd.env("DOCKER_BUILDKIT", "1");
    cmd.env("BUILDKIT_PROGRESS", "plain");

    run_command(cmd, &format!("docker build {package}"))
}

pub fn tag(config: &config::Config, package: &str) -> Result<()> {
    let image_name = get_image_name_for_package(config, package)?;
    let full_tags = full_tags_for_package(config, package)?;

    for full_tag in full_tags {
        println!("Tagging image {package} as {full_tag}");

        let mut cmd = Command::new("docker");
        cmd.arg("tag").arg(&image_name).arg(&full_tag);

        run_command(cmd, &format!("docker tag {package} -> {full_tag}"))?;
    }

    Ok(())
}

pub fn push(config: &config::Config, package: &str) -> Result<()> {
    let full_tags = full_tags_for_package(config, package)?;

    for full_tag in full_tags {
        println!("Pushing image: {full_tag}");

        let mut cmd = Command::new("docker");
        cmd.arg("push").arg(&full_tag);
        run_command(cmd, &format!("docker push {full_tag}"))?;
    }

    Ok(())
}

pub fn release(config: &config::Config, package: &str) -> Result<()> {
    build(config, package)?;
    tag(config, package)?;
    push(config, package)?;
    Ok(())
}

pub fn release_all(config: &config::Config) -> Result<()> {
    println!("Starting release-all...");
    process_all_packages(config, |package| release(config, package), "release")
}

pub fn build_all(config: &config::Config) -> Result<()> {
    println!("Starting build-all...");
    process_all_packages(config, |package| build(config, package), "build")
}

fn process_all_packages<F>(config: &config::Config, mut op: F, op_name: &str) -> Result<()>
where
    F: FnMut(&str) -> Result<()>,
{
    let mut failures = Vec::new();

    for (module, module_cfg) in &config.docker.modules {
        for package in &module_cfg.packages {
            println!("\n=== Processing {module}/{package} ===");

            if let Err(e) = op(package) {
                let error_msg = format!("{module}/{package}: {e}");
                eprintln!("❌ Failed to {op_name} {error_msg}");
                failures.push(error_msg);
            } else {
                println!("✅ Successfully {op_name}ed {module}/{package}");
            }
        }
    }

    if failures.is_empty() {
        println!("\n✅ Successfully {op_name}ed all packages");
        Ok(())
    } else {
        eprintln!(
            "\n❌ {op_name}-all completed with {} failures:",
            failures.len()
        );
        for failure in &failures {
            eprintln!("  - {failure}");
        }
        anyhow::bail!("{op_name}-all failed for {} packages", failures.len());
    }
}
