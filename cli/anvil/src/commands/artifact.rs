//! `anvil artifact release`: builds a program's apps and desktop packages and sends them to the warehouse.
//!
//! The tools that make the packages (`quench build apk --release`, `quench build deb`, `cargo build`) know nothing of
//! the warehouse; this is the one place that does, with the login anvil already keeps for the crates registry.
//!
//! The protocol is the warehouse's artifact registry: `PUT {warehouse}/api/v1/artifacts/{program}/{platform}/{version_code}`
//! with a bearer token and the file as the body. `201` is published; `409` means that version exists (they are
//! immutable). For an Android APK the warehouse reads the application id and version out of the manifest; for every
//! other platform it takes them from the URL and `?format=` is required.
//!
//! One version drives every build of a release: the version of `version_file`, and its [`version_code`], which is the
//! number `quench` puts in the APK too (the two are tested to agree).

use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use quench_core::{Context, Result, bail};
use quench_serde::Value;

use crate::config::{ArtifactImage, ArtifactProgram, ArtifactTarget, Config};

/// What a request to the warehouse carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upload {
    pub url: String,
    pub token: String,
    pub content_type: String,
    pub body: Vec<u8>,
}

/// What the warehouse said, as the status and the text of the body.
pub type Answer = (u16, String);

/// What `anvil artifact release` was asked.
#[derive(Debug, Clone, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each is a flag of the command, with its own meaning"
)]
pub struct Options {
    /// A target by name (`android`, `linux`, ...); every one when empty.
    pub target: Option<String>,
    /// Build and check everything, send nothing.
    pub dry_run: bool,
    /// Release from a tree with uncommitted changes.
    pub allow_dirty: bool,
    /// A version the warehouse already has is skipped, not an error (to finish a release that stopped half way).
    pub skip_existing: bool,
    /// Manage the version: see [`decide`]. A program whose version the warehouse has for every build is released
    /// as the next patch version; one it has for some builds is finished; one it does not have is released as it is.
    pub bump: bool,
    /// Builds the warehouse is known to hold already (set when finishing a release): they are not built again.
    pub have: Published,
}

/// `0.1.3` as `1003`, `1.2.3` as `1002003`: the number a store wants for a release.
///
/// The same arithmetic `quench` uses for the Android `versionCode`, so the APK and the desktop builds of one version
/// agree. A part over 999 is capped; anything that is not `major.minor.patch` counts as `0.0.0`.
#[must_use]
pub fn version_code(version: &str) -> u32 {
    let mut parts = version
        .split(['.', '-', '+'])
        .map(|p| p.parse::<u32>().unwrap_or(0));
    let (major, minor, patch) = (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0).min(999),
        parts.next().unwrap_or(0).min(999),
    );
    major
        .saturating_mul(1_000_000)
        .saturating_add(minor * 1_000 + patch)
}

/// The version in a `Cargo.toml`: the workspace's, else the package's.
///
/// # Errors
/// The file is unreadable or has no version.
pub fn read_version(path: &Path) -> Result<String> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value: Value = quench_serde::toml::from_str(&text)
        .with_context(|| format!("{} is not valid TOML", path.display()))?;
    let version = value
        .pointer("/workspace/package/version")
        .or_else(|| value.pointer("/package/version"))
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty());
    match version {
        Some(version) => Ok(version.to_string()),
        None => bail!(
            "{} has no version: set `version` under [workspace.package] or [package]",
            path.display()
        ),
    }
}

/// Replaces `${version_name}`, `${version_code}` and `${program}` in `text`.
#[must_use]
#[allow(
    clippy::literal_string_with_formatting_args,
    reason = "the placeholders are written like shell variables on purpose, and are not format arguments"
)]
pub fn expand(text: &str, program: &str, name: &str, code: u32) -> String {
    text.replace("${version_name}", name)
        .replace("${version_code}", &code.to_string())
        .replace("${program}", program)
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// The warehouse address of one build.
#[must_use]
pub fn upload_url(
    warehouse: &str,
    program: &str,
    target: &ArtifactTarget,
    code: u32,
    version_name: &str,
    filename: &str,
) -> String {
    let mut url = format!(
        "{}/api/v1/artifacts/{program}/{}/{code}",
        warehouse.trim_end_matches('/'),
        target.platform
    );
    // Android's identity is in the APK; every other platform's is asserted here.
    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(format) = target.format.as_deref().filter(|f| !f.is_empty()) {
        query.push(("format", format.to_string()));
    }
    if let Some(arch) = target.arch.as_deref().filter(|a| !a.is_empty()) {
        query.push(("arch", arch.to_string()));
    }
    query.push(("version_name", version_name.to_string()));
    if let Some(label) = target.label.as_deref().filter(|l| !l.is_empty()) {
        query.push(("label", label.to_string()));
    }
    query.push(("filename", filename.to_string()));
    url.push('?');
    url.push_str(
        &query
            .iter()
            .map(|(k, v)| format!("{k}={}", encode(v)))
            .collect::<Vec<_>>()
            .join("&"),
    );
    url
}

fn content_type(target: &ArtifactTarget) -> &'static str {
    match target.format.as_deref() {
        Some("apk") => "application/vnd.android.package-archive",
        _ if target.platform == "android" => "application/vnd.android.package-archive",
        _ => "application/octet-stream",
    }
}

/// A published build must be traceable to a commit: a dirty tree is refused. Returns the commit, if `dir` is in a
/// repository.
fn commit(dir: &Path, allow_dirty: bool) -> Result<Option<String>> {
    let git = |args: &[&str]| -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    if git(&["rev-parse", "--is-inside-work-tree"]).is_none() {
        return Ok(None);
    }
    let dirty = git(&["status", "--porcelain"]).unwrap_or_default();
    if !dirty.trim().is_empty() && !allow_dirty {
        let shown: Vec<&str> = dirty.lines().take(8).collect();
        bail!(
            "uncommitted changes:\n{}\ncommit them first, so the published build can be traced to a commit (or --allow-dirty)",
            shown.join("\n")
        );
    }
    let head = git(&["rev-parse", "--short", "HEAD"]).map(|h| h.trim().to_string());
    Ok(head.map(|h| {
        if dirty.trim().is_empty() {
            h
        } else {
            format!("{h}-dirty")
        }
    }))
}

fn run_build(
    dir: &Path,
    target: &ArtifactTarget,
    program: &str,
    name: &str,
    code: u32,
) -> Result<()> {
    let args: Vec<String> = target
        .build
        .iter()
        .map(|a| expand(a, program, name, code))
        .collect();
    let Some((command, rest)) = args.split_first() else {
        bail!("target `{}` has an empty `build`", target.platform);
    };
    let passwords = crate::commands::login::keystore_env(dir, |v| std::env::var_os(v).is_some())?;
    let status = Command::new(command)
        .args(rest)
        .current_dir(dir)
        .envs(passwords)
        .env("ANVIL_VERSION_NAME", name)
        .env("ANVIL_VERSION_CODE", code.to_string())
        .status()
        .with_context(|| format!("running `{command}`"))?;
    if !status.success() {
        bail!("`{}` failed ({status})", args.join(" "));
    }
    Ok(())
}

fn target_name(target: &ArtifactTarget) -> &str {
    target.name.as_deref().unwrap_or(&target.platform)
}

fn select<'a>(
    program: &'a ArtifactProgram,
    wanted: Option<&str>,
) -> Result<Vec<&'a ArtifactTarget>> {
    let chosen: Vec<&ArtifactTarget> = program
        .targets
        .iter()
        .filter(|t| wanted.is_none_or(|w| target_name(t) == w))
        .collect();
    if chosen.is_empty() {
        let names: Vec<&str> = program.targets.iter().map(target_name).collect();
        bail!(
            "no target {}: this program has {}",
            wanted.map_or_else(String::new, |w| format!("`{w}`")),
            if names.is_empty() {
                "none".to_string()
            } else {
                names.join(", ")
            }
        );
    }
    Ok(chosen)
}

/// Builds and sends one program, the way the real command does; the token and the transport are the caller's, so a
/// test can stand in for the warehouse.
///
/// # Errors
/// An unknown program or target, a dirty tree, a failed build, a missing file, a refusal from the warehouse.
#[allow(
    clippy::too_many_lines,
    reason = "one release is one sequence: check, build and send each target, then the image"
)]
pub fn release_with(
    config: &Config,
    root: &Path,
    name: &str,
    options: &Options,
    out: &mut dyn Write,
    token: &dyn Fn() -> Result<String>,
    put: &dyn Fn(&Upload) -> Result<Answer>,
) -> Result<()> {
    let program = config.artifacts.programs.get(name).with_context(|| {
        let known: Vec<&str> = config
            .artifacts
            .programs
            .keys()
            .map(String::as_str)
            .collect();
        format!(
            "no artifact `{name}` in the anvil configuration ({})",
            if known.is_empty() {
                "none are configured: add [artifacts.programs.<name>]".to_string()
            } else {
                format!("known: {}", known.join(", "))
            }
        )
    })?;
    let warehouse = config
        .artifacts
        .warehouse_url
        .as_deref()
        .filter(|u| !u.is_empty())
        .context("no warehouse: set `warehouse_url` under [artifacts] in .anvil.toml or .anvil/")?;
    let dir: PathBuf = root.join(program.dir.as_deref().unwrap_or("."));
    let version_name = read_version(&dir.join(&program.version_file))?;
    let code = version_code(&version_name);
    let only_image = options.target.as_deref() == Some("image") && program.image.is_some();
    let targets = if only_image || (program.targets.is_empty() && program.image.is_some()) {
        Vec::new()
    } else {
        select(program, options.target.as_deref())?
    };
    let release_image = program.image.is_some() && (only_image || options.target.is_none());
    let commit = commit(&dir, options.allow_dirty)?;
    writeln!(
        out,
        "{name} {version_name} (version_code {code}){} -> {warehouse}",
        commit
            .as_deref()
            .map(|c| format!(", commit {c}"))
            .unwrap_or_default()
    )?;

    let mut token_value: Option<String> = None;
    for target in targets {
        let label = target_name(target);
        if options
            .have
            .iter()
            .any(|(p, f, c)| *p == target.platform && f == format_of(target) && *c == code)
        {
            writeln!(out, "[{label}] already in the warehouse, not built again")?;
            continue;
        }
        writeln!(out, "[{label}] building: {}", target.build.join(" "))?;
        run_build(&dir, target, &program.program, &version_name, code)?;
        let file = dir.join(expand(&target.file, &program.program, &version_name, code));
        let body = std::fs::read(&file)
            .with_context(|| format!("[{label}] the build left no file at {}", file.display()))?;
        let filename = file
            .file_name()
            .and_then(|n| n.to_str())
            .context("the file has no name")?;
        let url = upload_url(
            warehouse,
            &program.program,
            target,
            code,
            &version_name,
            filename,
        );
        let what = format!(
            "{} {} {version_name} ({} KiB)",
            program.program,
            target.platform,
            body.len() / 1024
        );
        if options.dry_run {
            writeln!(out, "[{label}] would publish {what}\n    PUT {url}")?;
            continue;
        }
        if token_value.is_none() {
            token_value = Some(token()?);
        }
        writeln!(out, "[{label}] publishing {what}")?;
        let upload = Upload {
            url,
            token: token_value.clone().unwrap_or_default(),
            content_type: content_type(target).to_string(),
            body,
        };
        let (status, reply) = put(&upload)?;
        match status {
            201 => writeln!(out, "[{label}] published")?,
            409 if options.skip_existing => {
                writeln!(out, "[{label}] already published, skipped")?;
            }
            409 => bail!(
                "[{label}] version {version_name} (version_code {code}) is already published, and versions are immutable: raise the version (or --skip-existing to leave this one)"
            ),
            401 | 403 => bail!(
                "[{label}] the warehouse refused the login (HTTP {status}): `anvil login --scope warehouse:write` mints a token that may publish"
            ),
            _ => bail!("[{label}] publish failed: HTTP {status} {}", reply.trim()),
        }
    }
    if release_image && let Some(image) = &program.image {
        release_service_image(config, &dir, image, &version_name, options.dry_run, out)?;
    }
    Ok(())
}

/// Builds and pushes the program's service image, tagged with the release's version.
fn release_service_image(
    config: &Config,
    dir: &Path,
    image: &ArtifactImage,
    version_name: &str,
    dry_run: bool,
    out: &mut dyn Write,
) -> Result<()> {
    let registries: Vec<&str> = if image.registries.is_empty() {
        vec![config.docker.registry.as_str()]
    } else {
        image.registries.iter().map(String::as_str).collect()
    };
    let tags: Vec<String> = registries
        .iter()
        .filter(|r| !r.trim().is_empty())
        .map(|r| format!("{r}/{}/{}:{version_name}", image.module, image.name))
        .collect();
    if tags.is_empty() {
        bail!(
            "[image] no registry: set `registries` on the image or `registry` under [docker] in .anvil.toml or .anvil/"
        );
    }
    let dockerfile = dir.join(&image.dockerfile);
    let context = dir.join(image.context.as_deref().unwrap_or("."));
    if !dockerfile.is_file() {
        bail!("[image] no Dockerfile at {}", dockerfile.display());
    }
    if dry_run {
        writeln!(
            out,
            "[image] would build {} and push {}",
            dockerfile.display(),
            tags.join(", ")
        )?;
        return Ok(());
    }
    writeln!(out, "[image] building and pushing {}", tags.join(", "))?;
    crate::commands::docker::release_image(
        config,
        &dockerfile,
        &context,
        &image.build_args,
        &tags,
    )?;
    writeln!(out, "[image] pushed")?;
    Ok(())
}

/// A build the warehouse holds: `(platform, format, version_code)`.
pub type Published = Vec<(String, String, u32)>;

/// What the warehouse says about a program's builds: the answer to `GET .../api/v1/artifacts/{program}`.
///
/// # Errors
/// The text is not the list the warehouse sends.
pub fn parse_published(text: &str) -> Result<Published> {
    let value: Value = quench_serde::json::from_str(text)
        .context("the warehouse's list of versions is not JSON")?;
    let Value::Array(items) = value else {
        bail!("the warehouse's list of versions is not a list");
    };
    Ok(items
        .iter()
        .filter_map(|item| {
            Some((
                item.get("platform")?.as_str()?.to_string(),
                item.get("format")?.as_str()?.to_string(),
                u32::try_from(item.get("version_code")?.as_i64()?).ok()?,
            ))
        })
        .collect())
}

/// What the version of a program is to do before a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Version {
    /// Not in the warehouse: release it as it is.
    Keep,
    /// Some builds are there and some are not (a release that stopped half way): send what is missing.
    Finish,
    /// Every build is there: raise the patch version.
    Bump { from: String, to: String },
}

/// The format a target is published under: the one it says, else `apk` for Android.
fn format_of(target: &ArtifactTarget) -> &str {
    target
        .format
        .as_deref()
        .unwrap_or(if target.platform == "android" {
            "apk"
        } else {
            ""
        })
}

/// Decides what to do with the version of `program` given what the warehouse holds. Versions are immutable there,
/// so a version that is fully published can only be released as a new one.
///
/// # Errors
/// The version is not `MAJOR.MINOR.PATCH` (and so cannot be raised).
pub fn decide(
    program: &ArtifactProgram,
    version: &str,
    published: &Published,
    only: Option<&str>,
) -> Result<Version> {
    let code = version_code(version);
    let targets = select(program, only).unwrap_or_default();
    if targets.is_empty() {
        return Ok(Version::Keep);
    }
    let there = targets
        .iter()
        .filter(|t| {
            published
                .iter()
                .any(|(p, f, c)| *p == t.platform && f == format_of(t) && *c == code)
        })
        .count();
    Ok(if there == 0 {
        Version::Keep
    } else if there < targets.len() {
        Version::Finish
    } else {
        Version::Bump {
            from: version.to_string(),
            to: crate::commands::release::bump_patch(version)?,
        }
    })
}

/// Writes `version` into the `version_file` of a program (its `[package]`, else its `[workspace.package]`).
///
/// # Errors
/// The file cannot be read or written, or has no version to replace.
pub fn set_version(path: &Path, version: &str) -> Result<()> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let release = crate::commands::release::with_table_version;
    let updated = release(&text, "[package]", version)
        .or_else(|| release(&text, "[workspace.package]", version))
        .with_context(|| format!("{} has no version to raise", path.display()))?;
    std::fs::write(path, updated).with_context(|| format!("writing {}", path.display()))
}

/// Commits the raised version (and the `Cargo.lock` that follows it) in the repository `dir` is in. `Ok(false)`
/// when `dir` is not in one.
fn commit_version(dir: &Path, file: &Path, message: &str) -> Result<bool> {
    let git = |args: &[&str]| -> Result<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .context("running git")?;
        if !out.status.success() {
            bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let Ok(top) = git(&["rev-parse", "--show-toplevel"]) else {
        return Ok(false);
    };
    // The lock file follows the version once cargo has looked at the workspace again.
    let _ = Command::new("cargo")
        .args(["update", "--workspace", "--offline"])
        .current_dir(dir)
        .output();
    let mut paths = vec![file.to_path_buf()];
    let lock = PathBuf::from(top.trim()).join("Cargo.lock");
    if lock.is_file()
        && !git(&["status", "--porcelain", "--", &lock.to_string_lossy()])?
            .trim()
            .is_empty()
    {
        paths.push(lock);
    }
    let names: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let mut add = vec!["add", "--"];
    add.extend(names.iter().map(String::as_str));
    git(&add)?;
    let mut commit = vec!["commit", "-m", message, "--"];
    commit.extend(names.iter().map(String::as_str));
    git(&commit)?;
    Ok(true)
}

/// Asks the warehouse which builds of a program it holds, over the real network.
pub fn list_published(url: &str, token: &str) -> Result<Answer> {
    let response = match quench_client::blocking::Client::new()
        .get(url)
        .bearer_auth(token)
        .send()
    {
        Ok(response) => response,
        Err(e) => bail!("the warehouse could not be reached: {e}"),
    };
    let status = response.status().as_u16();
    Ok((status, response.text().unwrap_or_default()))
}

/// The address of a program's list of builds.
#[must_use]
pub fn list_url(warehouse: &str, program: &str) -> String {
    format!(
        "{}/api/v1/artifacts/{}",
        warehouse.trim_end_matches('/'),
        encode(program)
    )
}

/// Releases one program with its version looked after: see [`Options::bump`]. The transport is the caller's.
///
/// # Errors
/// What [`release_with`] says, and a warehouse that cannot say what it holds.
#[allow(clippy::too_many_arguments)]
pub fn release_managed(
    config: &Config,
    root: &Path,
    name: &str,
    options: &Options,
    out: &mut dyn Write,
    token: &dyn Fn() -> Result<String>,
    put: &dyn Fn(&Upload) -> Result<Answer>,
    list: &dyn Fn(&str, &str) -> Result<Answer>,
) -> Result<()> {
    if !options.bump {
        return release_with(config, root, name, options, out, token, put);
    }
    let program = config
        .artifacts
        .programs
        .get(name)
        .with_context(|| format!("no artifact `{name}` in the anvil configuration"))?;
    let warehouse = config
        .artifacts
        .warehouse_url
        .as_deref()
        .filter(|u| !u.is_empty())
        .context("no warehouse: set `warehouse_url` under [artifacts] in .anvil.toml or .anvil/")?;
    let dir = root.join(program.dir.as_deref().unwrap_or("."));
    let version_file = dir.join(&program.version_file);
    let version = read_version(&version_file)?;
    let (status, body) = list(&list_url(warehouse, &program.program), &token()?)?;
    if status != 200 {
        bail!(
            "{name}: the warehouse could not say which versions it holds (HTTP {status}); it is needed to look after the version, or leave that to yourself without --bump"
        );
    }
    let published = parse_published(&body)?;
    match decide(program, &version, &published, options.target.as_deref())? {
        Version::Keep => release_with(config, root, name, options, out, token, put),
        Version::Finish => {
            writeln!(
                out,
                "{name} {version}: some builds are in the warehouse, the others are sent now (a release that stopped half way)"
            )?;
            let finish = Options {
                skip_existing: true,
                have: published,
                ..options.clone()
            };
            release_with(config, root, name, &finish, out, token, put)
        }
        Version::Bump { from, to } => {
            if options.dry_run {
                writeln!(
                    out,
                    "{name} {from}: the warehouse has every build of it; a real run would release {to} (nothing is built or changed now)"
                )?;
                return Ok(());
            }
            // Do not raise a version on a tree that could not be released anyway.
            commit(&dir, options.allow_dirty)?;
            set_version(&version_file, &to)?;
            let committed = commit_version(&dir, &version_file, &format!("release: {name} {to}"))?;
            writeln!(
                out,
                "{name} {from}: the warehouse has every build of it; raised to {to}{}",
                if committed { " and committed" } else { "" }
            )?;
            release_with(config, root, name, options, out, token, put)
        }
    }
}

/// Releases every program of the configuration, each looked after by [`release_managed`], in name order. Stops at
/// the first that fails. `confirm` is asked once, unless `yes`.
///
/// # Errors
/// No programs, a refused confirmation that cannot be asked, or the first program that fails.
#[allow(clippy::too_many_arguments)]
pub fn release_all_with(
    config: &Config,
    root: &Path,
    options: &Options,
    yes: bool,
    out: &mut dyn Write,
    confirm: &dyn Fn() -> Result<bool>,
    token: &dyn Fn() -> Result<String>,
    put: &dyn Fn(&Upload) -> Result<Answer>,
    list: &dyn Fn(&str, &str) -> Result<Answer>,
) -> Result<()> {
    let names: Vec<&String> = config.artifacts.programs.keys().collect();
    if names.is_empty() {
        bail!("no artifacts: add [artifacts.programs.<name>] to .anvil.toml or a file in .anvil/");
    }
    writeln!(
        out,
        "{} program(s): {}{}",
        names.len(),
        names
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        if options.dry_run { " (dry run)" } else { "" }
    )?;
    if !yes && !options.dry_run && !confirm()? {
        writeln!(out, "Release cancelled.")?;
        return Ok(());
    }
    for name in names {
        writeln!(out, "\n== {name}")?;
        release_managed(config, root, name, options, out, token, put, list)
            .with_context(|| format!("releasing {name}"))?;
    }
    Ok(())
}

/// `anvil artifact release --all`.
///
/// # Errors
/// See [`release_all_with`].
pub fn release_all(config: &Config, options: &Options, yes: bool) -> Result<()> {
    let root = std::env::current_dir().context("finding the working directory")?;
    let token_root = root.clone();
    release_all_with(
        config,
        &root,
        options,
        yes,
        &mut std::io::stdout(),
        &|| {
            print!("Proceed with the release? (yes/no): ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            if std::io::stdin().read_line(&mut input)? == 0 {
                bail!(
                    "no answer to the confirmation (standard input is closed): pass --yes to release without asking"
                );
            }
            Ok(matches!(
                input.trim().to_ascii_lowercase().as_str(),
                "yes" | "y"
            ))
        },
        &move || token(&token_root),
        &send,
        &list_published,
    )
}

/// The login anvil keeps, as the bare token the warehouse is sent.
fn token(root: &Path) -> Result<String> {
    let bearer = crate::commands::login::registry_token(root, |n| std::env::var(n).ok())?;
    Ok(bearer.trim_start_matches("Bearer ").to_string())
}

/// Sends one file with a bearer token, over the real network.
pub fn send(upload: &Upload) -> Result<Answer> {
    let sent = quench_client::blocking::Client::new()
        .put(&upload.url)
        .bearer_auth(&upload.token)
        .header("content-type", &upload.content_type)
        .body(upload.body.clone())
        .send();
    let response = match sent {
        Ok(response) => response,
        Err(e) => bail!("the warehouse could not be reached: {e}"),
    };
    let status = response.status().as_u16();
    Ok((status, response.text().unwrap_or_default()))
}

/// `anvil artifact release`.
///
/// # Errors
/// See [`release_with`].
pub fn release(config: &Config, name: &str, options: &Options) -> Result<()> {
    let root = std::env::current_dir().context("finding the working directory")?;
    let token_root = root.clone();
    release_managed(
        config,
        &root,
        name,
        options,
        &mut std::io::stdout(),
        // `Bearer <token>` as Cargo wants it; the warehouse is sent the bare token.
        &move || token(&token_root),
        &send,
        &list_published,
    )
}

/// `anvil artifact list`: what can be released and what it would build.
///
/// # Errors
/// Output failed.
pub fn list(config: &Config, out: &mut dyn Write) -> Result<()> {
    if config.artifacts.programs.is_empty() {
        writeln!(
            out,
            "no artifacts: add [artifacts.programs.<name>] to .anvil.toml or a file in .anvil/"
        )?;
        return Ok(());
    }
    for (name, program) in &config.artifacts.programs {
        writeln!(out, "{name} ({})", program.program)?;
        for t in &program.targets {
            writeln!(
                out,
                "  {:<10} {} {}",
                target_name(t),
                t.format.as_deref().unwrap_or("-"),
                t.arch.as_deref().unwrap_or("-")
            )?;
        }
        if let Some(image) = &program.image {
            writeln!(out, "  {:<10} {}/{}", "image", image.module, image.name)?;
        }
    }
    Ok(())
}
