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

use crate::config::{ArtifactProgram, ArtifactTarget, Config};

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
pub struct Options {
    /// A target by name (`android`, `linux`, ...); every one when empty.
    pub target: Option<String>,
    /// Build and check everything, send nothing.
    pub dry_run: bool,
    /// Release from a tree with uncommitted changes.
    pub allow_dirty: bool,
    /// A version the warehouse already has is skipped, not an error (to finish a release that stopped half way).
    pub skip_existing: bool,
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
    let status = Command::new(command)
        .args(rest)
        .current_dir(dir)
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
            "no artifact `{name}` in .anvil.toml ({})",
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
        .context("no warehouse: set `warehouse_url` under [artifacts] in .anvil.toml")?;
    let dir: PathBuf = root.join(program.dir.as_deref().unwrap_or("."));
    let version_name = read_version(&dir.join(&program.version_file))?;
    let code = version_code(&version_name);
    let targets = select(program, options.target.as_deref())?;
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
    Ok(())
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
    release_with(
        config,
        &root,
        name,
        options,
        &mut std::io::stdout(),
        &move || {
            // `Bearer <token>` as Cargo wants it; the warehouse is sent the bare token.
            let bearer =
                crate::commands::login::registry_token(&token_root, |n| std::env::var(n).ok())?;
            Ok(bearer.trim_start_matches("Bearer ").to_string())
        },
        &send,
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
            "no artifacts: add [artifacts.programs.<name>] to .anvil.toml"
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
    }
    Ok(())
}
