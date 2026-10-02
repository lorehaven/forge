//! The package commands as `main` runs them: the glue between argument
//! parsing, [`crate::package`] and [`crate::registry`], and what gets printed.

use crate::cli::{ApplyScope, RemoteCmd};
use crate::env::with_workspace;
use crate::image_updates::{RegistryDigests, print_rows};
use crate::package::{
    InstallValues, PackOptions, Packed, check_requirements, merge_values, pack, prepare_install,
    read_file,
};
use crate::registry::{Registry, RemotePackage};
use crate::render::{ResourceScope, Selector};
use crate::repl::{WaitPolicy, describe, kubectl_apply, ok, warn};
use anyhow::{Context as _, Result, bail, ensure};
use rivet_package::{Limits, Package};
use std::path::{Path, PathBuf};

/// A package as the user names it.
#[derive(Debug, PartialEq, Eq)]
pub enum PackageRef {
    /// `name` or `name@version`, to fetch from Warehouse.
    Remote {
        /// Package name.
        name: String,
        /// `None` means the newest.
        version: Option<String>,
    },
    /// A `.rivet` file on disk.
    File(PathBuf),
}

/// Reads `name`, `name@version` or a path.
///
/// Anything that ends in `.rivet`, or names an existing file, is a path - a
/// package name can never contain a dot, so the two cannot be confused.
pub fn parse_package_ref(value: &str) -> Result<PackageRef> {
    let is_rivet = Path::new(value)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(rivet_package::EXTENSION));
    if is_rivet || Path::new(value).is_file() {
        return Ok(PackageRef::File(PathBuf::from(value)));
    }

    let (name, version) = match value.split_once('@') {
        Some((name, version)) => (name, Some(version)),
        None => (value, None),
    };
    ensure!(
        rivet_package::manifest::is_valid_name(name),
        "`{value}` is neither a .rivet file nor a package name (lowercase a-z, 0-9, `-`)"
    );
    if let Some(version) = version
        && version != "latest"
    {
        semver::Version::parse(version)
            .with_context(|| format!("`{version}` is not a semver version (or `latest`)"))?;
    }

    Ok(PackageRef::Remote {
        name: name.to_string(),
        version: version.filter(|v| *v != "latest").map(str::to_string),
    })
}

fn describe_size(bytes: usize) -> String {
    #[allow(clippy::cast_precision_loss)]
    let kib = bytes as f64 / 1024.0;
    format!("{kib:.1} KiB")
}

fn report_packed(packed: &Packed) {
    ok(&format!(
        "packed {} {} -> {} ({}, sha256 {})",
        packed.manifest.package.name,
        packed.manifest.package.version,
        packed.path.display(),
        describe_size(packed.size_bytes),
        &packed.sha256[..12],
    ));
    if !packed.pinned.is_empty() {
        ok(&format!(
            "pinned {} image(s) to digests",
            packed.pinned.len()
        ));
    }
    for note in &packed.unpinned {
        warn(&format!("not pinned: {note}"));
    }
    if !packed.excluded.is_empty() {
        ok(&format!("left out: {}", packed.excluded.join(", ")));
    }
    if !packed.required_vars.is_empty() {
        ok(&format!(
            "installing needs these variables (no default in values.toml): {}",
            packed.required_vars.join(", ")
        ));
    }
}

/// The suffix with its tokens expanded against the clock and the commit the CI run is for.
fn resolve_suffix(template: Option<&str>) -> Result<Option<String>> {
    let sha = ["CONVEYOR_SHA", "GITHUB_SHA"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.trim().is_empty()));
    template
        .map(|t| crate::package::expand_suffix(t, chrono::Utc::now(), sha.as_deref()))
        .transpose()
}

/// `riveter pack`.
pub fn pack_command(
    env: &str,
    version_suffix: Option<&str>,
    no_pin: bool,
    out: &Path,
    registry_auth: &[String],
) -> Result<Packed> {
    let resolver = (!no_pin).then(|| RegistryDigests::new(registry_auth));
    let version_suffix = resolve_suffix(version_suffix)?;
    let packed = pack(
        &PackOptions {
            env,
            overlays_dir: &crate::env::overlay_dir(),
            out_dir: out,
            version_suffix: version_suffix.as_deref(),
        },
        resolver
            .as_ref()
            .map(|r| r as &dyn crate::package::DigestResolver),
    )?;
    report_packed(&packed);
    Ok(packed)
}

/// `riveter publish`.
pub fn publish_command(
    env: Option<&str>,
    file: Option<&Path>,
    version_suffix: Option<&str>,
    no_pin: bool,
    out: &Path,
    registry_auth: &[String],
) -> Result<()> {
    let registry = Registry::from_env()?;

    let path = if let Some(file) = file {
        file.to_path_buf()
    } else {
        let env = env.context("name an environment to pack, or give a .rivet file")?;
        pack_command(env, version_suffix, no_pin, out, registry_auth)?.path
    };

    let package = read_file(&path)?;
    let bytes =
        std::fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let record = registry.publish(
        &package.manifest.package.name,
        &package.manifest.package.version,
        bytes,
    )?;
    ok(&format!(
        "published {} {} (sha256 {})",
        record.name,
        record.version,
        &record.sha256[..12.min(record.sha256.len())]
    ));
    Ok(())
}

/// Fetches and validates a package named on the command line.
fn load_package(reference: &PackageRef) -> Result<(Package, String)> {
    match reference {
        PackageRef::File(path) => Ok((read_file(path)?, path.display().to_string())),
        PackageRef::Remote { name, version } => {
            let fetched =
                Registry::from_env()?.fetch(name, version.as_deref().unwrap_or("latest"))?;
            let package = Package::read(std::io::Cursor::new(&fetched.bytes), &Limits::default())
                .with_context(|| {
                format!("{name} {} is not a valid package", fetched.version)
            })?;
            Ok((package, format!("Warehouse ({name} {})", fetched.version)))
        }
    }
}

/// `riveter pull`.
pub fn pull_command(package: &str, out: &Path) -> Result<()> {
    let reference = parse_package_ref(package)?;
    let PackageRef::Remote { name, version } = reference else {
        bail!("`{package}` is a file; pull takes a package name");
    };

    let fetched = Registry::from_env()?.fetch(&name, version.as_deref().unwrap_or("latest"))?;
    Package::read(std::io::Cursor::new(&fetched.bytes), &Limits::default())
        .with_context(|| format!("{name} {} is not a valid package", fetched.version))?;

    std::fs::create_dir_all(out)?;
    let path = out.join(rivet_package::file_name(&name, &fetched.version));
    std::fs::write(&path, &fetched.bytes)
        .with_context(|| format!("failed to write {}", path.display()))?;
    ok(&format!(
        "pulled {name} {} -> {}",
        fetched.version,
        path.display()
    ));
    Ok(())
}

/// Everything `install` takes from the command line.
#[derive(Debug)]
pub struct InstallArgs<'a> {
    /// `name[@version]` or a file.
    pub package: &'a str,
    /// `--env-file`.
    pub env_file: Option<&'a Path>,
    /// `--set`.
    pub sets: &'a [String],
    /// `--dry-run`.
    pub dry_run: bool,
    /// Rollout waiting.
    pub wait: WaitPolicy,
    /// `--scope`.
    pub scope: ApplyScope,
    /// Resource targets.
    pub targets: &'a [String],
}

/// `riveter install`.
pub fn install_command(args: &InstallArgs<'_>) -> Result<()> {
    let selector = Selector::parse(args.targets)?;
    let reference = parse_package_ref(args.package)?;
    let (package, source) = load_package(&reference)?;

    for note in check_requirements(&package.manifest, env!("CARGO_PKG_VERSION"))? {
        warn(&note);
    }

    let vars = merge_values(
        &package,
        &InstallValues {
            env_file: args.env_file,
            sets: args.sets,
        },
    )?;
    let installation = prepare_install(&package, vars)?;

    ok(&format!(
        "installing {} {} from {source}",
        package.manifest.package.name, package.manifest.package.version
    ));

    let scope = match args.scope {
        ApplyScope::Mutable => ResourceScope::Mutable,
        ApplyScope::Immutable => ResourceScope::Immutable,
        ApplyScope::All => ResourceScope::All,
    };
    let rendered = with_workspace(installation.workspace.clone(), || {
        kubectl_apply(&installation.env, args.dry_run, scope, &selector, args.wait)
    })?;

    if rendered.resource_count == 0 {
        ok("no resources matched selected scope");
    } else {
        let verb = if args.dry_run {
            "would apply"
        } else {
            "applied"
        };
        ok(&format!(
            "{verb} {} resource(s): {}",
            rendered.resource_count,
            describe(&rendered)
        ));
    }
    Ok(())
}

fn rows(packages: &[RemotePackage], status: impl Fn(&RemotePackage) -> String) -> Vec<[String; 4]> {
    packages
        .iter()
        .map(|p| {
            [
                p.name.clone(),
                p.version.clone(),
                status(p),
                format!("{} KiB", p.size_bytes.div_ceil(1024)),
            ]
        })
        .collect()
}

/// `riveter remote`.
pub fn remote_command(cmd: &RemoteCmd) -> Result<()> {
    let registry = Registry::from_env()?;
    match cmd {
        RemoteCmd::List => {
            let packages = registry.catalog()?;
            if packages.is_empty() {
                ok("the registry holds no packages");
                return Ok(());
            }
            print_rows(
                "name, version, description, size:",
                &packages
                    .iter()
                    .map(|p| {
                        [
                            p.name.clone(),
                            p.version.clone(),
                            p.description.clone().unwrap_or_default(),
                            format!("{} KiB", p.size_bytes.div_ceil(1024)),
                        ]
                    })
                    .collect::<Vec<_>>(),
            );
        }
        RemoteCmd::Versions { name } => {
            let packages = registry.versions(name)?;
            if packages.is_empty() {
                ok(&format!("no versions of `{name}`"));
                return Ok(());
            }
            print_rows(
                "name, version, status, size:",
                &rows(&packages, |p| {
                    if p.yanked { "yanked" } else { "available" }.to_string()
                }),
            );
        }
    }
    Ok(())
}
