use anyhow::{Context, Result};
use std::fs;
use std::process::Command;

use crate::util::run_command;

fn get_modules() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "api",
            vec![
                "access-control",
                "audit",
                "gdrive-api",
                "job-manager",
                "knowledge-base",
                "mailbox",
                "quiz-manager",
                "worker",
            ],
        ),
        ("jobs", vec!["gdrive-sync", "postgres-init"]),
        ("web", vec!["frontend"]),
    ]
}

fn get_skipped_modules() -> Vec<&'static str> {
    vec!["libs", "tools"]
}

fn find_module_for_package(package: &str) -> Result<String> {
    for (module, packages) in get_modules() {
        if packages.contains(&package) {
            return Ok(module.to_string());
        }
    }

    for module in get_skipped_modules() {
        let path = format!("{}/{}", module, package);
        if std::path::Path::new(&path).exists() {
            anyhow::bail!(
                "Package '{}' is in module '{}' which is skipped for Docker operations",
                package,
                module
            );
        }
    }

    anyhow::bail!("Module not found for package: {}", package)
}

fn get_package_version(module: &str, package: &str) -> Result<String> {
    let path = format!("{}/{}/Cargo.toml", module, package);
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read Cargo.toml at {}", path))?;

    let value: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Failed to parse Cargo.toml at {}", path))?;

    value
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .context(format!("Version not found in {}", path))
}

fn get_dockerfile_for_package(package: &str) -> Result<String> {
    let module = find_module_for_package(package)?;
    if module == "web" {
        Ok("docker/Dockerfile.alpine-dx".to_string())
    } else if module == "api" && package == "worker" {
        Ok("docker/Dockerfile.ubuntu".to_string())
    } else {
        Ok("docker/Dockerfile.alpine".to_string())
    }
}

pub fn build(package: &str) -> Result<()> {
    let dockerfile = get_dockerfile_for_package(package)?;
    println!(
        "Building Docker image for package: {} using {}",
        package, dockerfile
    );

    let mut cmd = Command::new("docker");
    cmd.arg("build")
        .arg("-f")
        .arg(&dockerfile)
        .arg("--build-arg")
        .arg(format!("PROJECT_NAME={}", package))
        .arg("-t")
        .arg(package)
        .arg(".");

    // Enable BuildKit
    cmd.env("DOCKER_BUILDKIT", "1");

    run_command(cmd, &format!("docker build {}", package))
}

pub fn tag(package: &str, registry: &str) -> Result<()> {
    let module = find_module_for_package(package)?;
    let version = get_package_version(&module, package)?;

    let full_tag = format!("{}/{}/{}:{}", registry, module, package, version);
    println!("Tagging image {} as {}", package, full_tag);

    let mut cmd = Command::new("docker");
    cmd.arg("tag").arg(package).arg(&full_tag);

    run_command(cmd, &format!("docker tag {}", package))
}

pub fn push(package: &str, registry: &str) -> Result<()> {
    let module = find_module_for_package(package)?;
    let version = get_package_version(&module, package)?;

    let full_tag = format!("{}/{}/{}:{}", registry, module, package, version);
    println!("Pushing image: {}", full_tag);

    let mut cmd = Command::new("docker");
    cmd.arg("push").arg(&full_tag);

    run_command(cmd, &format!("docker push {}", package))
}

pub fn release(package: &str, registry: &str) -> Result<()> {
    build(package)?;
    tag(package, registry)?;
    push(package, registry)?;
    Ok(())
}

pub fn release_all(registry: &str) -> Result<()> {
    println!("Starting release-all...");

    // Track failures
    let mut failures = Vec::new();

    for (module, packages) in get_modules() {
        for package in packages {
            println!("\n=== Processing {}/{} ===", module, package);

            if let Err(e) = release(package, registry) {
                let error_msg = format!("{}/{}: {}", module, package, e);
                eprintln!("❌ Failed to release {}", error_msg);
                failures.push(error_msg);
            } else {
                println!("✅ Successfully released {}/{}", module, package);
            }
        }
    }

    if !failures.is_empty() {
        eprintln!(
            "\n❌ Release-all completed with {} failures:",
            failures.len()
        );
        for failure in &failures {
            eprintln!("  - {}", failure);
        }
        anyhow::bail!("Release-all failed for {} packages", failures.len());
    }

    println!("\n✅ Successfully released all packages");
    Ok(())
}

pub fn build_all() -> Result<()> {
    println!("Starting build-all...");

    // Track failures
    let mut failures = Vec::new();

    for (module, packages) in get_modules() {
        for package in packages {
            println!("\n=== Building {}/{} ===", module, package);

            if let Err(e) = build(package) {
                let error_msg = format!("{}/{}: {}", module, package, e);
                eprintln!("❌ Failed to build {}", error_msg);
                failures.push(error_msg);
            } else {
                println!("✅ Successfully built {}/{}", module, package);
            }
        }
    }

    if !failures.is_empty() {
        eprintln!("\n❌ Build-all completed with {} failures:", failures.len());
        for failure in &failures {
            eprintln!("  - {}", failure);
        }
        anyhow::bail!("Build-all failed for {} packages", failures.len());
    }

    println!("\n✅ Successfully built all packages");
    Ok(())
}
