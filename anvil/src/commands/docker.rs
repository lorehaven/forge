use crate::config;
use crate::util::run_command;
use anyhow::{Context, Result};
use std::fs;
use std::process::Command;

fn find_module_for_package<'a>(config: &'a config::Config, package: &str) -> Result<&'a str> {
    for (module, module_cfg) in &config.docker.modules {
        if module_cfg.packages.iter().any(|p| p == package) {
            return Ok(module);
        }
    }

    anyhow::bail!("Module not found for package: {package}")
}

fn get_package_version(module: &str, package: &str) -> Result<String> {
    let path = format!("{module}/{package}/Cargo.toml");
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read Cargo.toml at {path}"))?;

    let value: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Failed to parse Cargo.toml at {path}"))?;

    value
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(toml::Value::as_str)
        .map(ToString::to_string)
        .with_context(|| format!("Version not found in {path}"))
}

fn get_dockerfile_for_package(config: &config::Config, package: &str) -> Result<String> {
    let module = find_module_for_package(config, package)?;
    let module_cfg = &config.docker.modules[module];

    Ok(if let Some(dockerfile) = module_cfg
        .package_overrides
        .get(package)
        .and_then(|override_cfg| override_cfg.dockerfile.as_ref())
    {
        dockerfile.clone()
    } else {
        module_cfg.dockerfile.clone()
    })
}

fn get_image_name_for_package(config: &config::Config, package: &str) -> Result<String> {
    let module = find_module_for_package(config, package)?;
    let module_cfg = &config.docker.modules[module];

    Ok(if let Some(image_name) = module_cfg
        .package_overrides
        .get(package)
        .and_then(|override_cfg| override_cfg.image_name.as_ref())
    {
        image_name.clone()
    } else {
        package.to_string()
    })
}

fn get_registries_for_package(config: &config::Config, package: &str) -> Result<Vec<String>> {
    let module = find_module_for_package(config, package)?;
    let module_cfg = &config.docker.modules[module];

    let registries = if let Some(override_cfg) = module_cfg.package_overrides.get(package) {
        if !override_cfg.registries.is_empty() {
            override_cfg.registries.clone()
        } else if let Some(registry) = override_cfg.registry.as_ref() {
            vec![registry.clone()]
        } else if !config.docker.registry.trim().is_empty() {
            vec![config.docker.registry.clone()]
        } else {
            Vec::new()
        }
    } else if !config.docker.registry.trim().is_empty() {
        vec![config.docker.registry.clone()]
    } else {
        Vec::new()
    };

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

fn full_tags_for_package(config: &config::Config, package: &str) -> Result<Vec<String>> {
    let registries = get_registries_for_package(config, package)?;
    let module = find_module_for_package(config, package)?;
    let image_name = get_image_name_for_package(config, package)?;
    let version = get_package_version(module, package)?;
    Ok(registries
        .iter()
        .map(|registry| format!("{registry}/{module}/{image_name}:{version}"))
        .collect())
}

pub fn build(config: &config::Config, package: &str) -> Result<()> {
    let dockerfile = get_dockerfile_for_package(config, package)?;
    let image_name = get_image_name_for_package(config, package)?;
    let module = find_module_for_package(config, package)?;
    let resources_path = format!("{module}/{package}");
    println!("Building Docker image for package: {package} using {dockerfile}");

    let mut cmd = Command::new("docker");
    cmd.arg("build")
        .arg("-f")
        .arg(&dockerfile)
        .arg("--build-arg")
        .arg(format!("PROJECT_NAME={package}"))
        .arg("--build-arg")
        .arg(format!("RESOURCES_PATH={resources_path}"))
        .arg("-t")
        .arg(image_name)
        .arg(".");

    // Enable BuildKit
    cmd.env("DOCKER_BUILDKIT", "1");

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
