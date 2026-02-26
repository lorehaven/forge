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

    module_cfg
        .package_dockerfiles
        .get(package)
        .map_or_else(|| Ok(module_cfg.dockerfile.clone()), |df| Ok(df.clone()))
}

fn get_image_name_for_package(config: &config::Config, package: &str) -> Result<String> {
    let module = find_module_for_package(config, package)?;
    let module_cfg = &config.docker.modules[module];

    module_cfg
        .package_images
        .get(package)
        .map_or_else(|| Ok(package.to_string()), |im| Ok(im.clone()))
}

pub fn build(config: &config::Config, package: &str) -> Result<()> {
    let dockerfile = get_dockerfile_for_package(config, package)?;
    let image_name = get_image_name_for_package(config, package)?;
    println!("Building Docker image for package: {package} using {dockerfile}");

    let mut cmd = Command::new("docker");
    cmd.arg("build")
        .arg("-f")
        .arg(&dockerfile)
        .arg("--build-arg")
        .arg(format!("PROJECT_NAME={package}"))
        .arg("-t")
        .arg(image_name)
        .arg(".");

    // Enable BuildKit
    cmd.env("DOCKER_BUILDKIT", "1");

    run_command(cmd, &format!("docker build {package}"))
}

pub fn tag(config: &config::Config, package: &str) -> Result<()> {
    let registry = &config.docker.registry;
    let module = find_module_for_package(config, package)?;
    let image_name = get_image_name_for_package(config, package)?;
    let version = get_package_version(module, package)?;

    let full_tag = format!("{registry}/{module}/{image_name}:{version}");
    println!("Tagging image {package} as {full_tag}");

    let mut cmd = Command::new("docker");
    cmd.arg("tag").arg(package).arg(&full_tag);

    run_command(cmd, &format!("docker tag {package}"))
}

pub fn push(config: &config::Config, package: &str) -> Result<()> {
    let registry = &config.docker.registry;
    let module = find_module_for_package(config, package)?;
    let image_name = get_image_name_for_package(config, package)?;
    let version = get_package_version(module, package)?;

    let full_tag = format!("{registry}/{module}/{image_name}:{version}");
    println!("Pushing image: {full_tag}");

    let mut cmd = Command::new("docker");
    cmd.arg("push").arg(&full_tag);

    run_command(cmd, &format!("docker push {full_tag}"))
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
