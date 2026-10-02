//! Building a `.rivet` package from an overlay directory, and laying one back
//! out for install.
//!
//! A package is exactly one overlay directory - `overlays/forge/` becomes the
//! package `forge`, with `overlay.yaml` and everything it includes at the
//! archive root. The directory name is part of the package because the overlay's
//! own `{% include "forge/base.yaml.j2" %}` lines spell it out, so an install
//! puts the files back under `overlays/<name>/` and nothing needs rewriting.
//!
//! Nothing here talks to a cluster. The registry client is [`crate::registry`];
//! the only network access in this module is the digest lookup, and that sits
//! behind [`DigestResolver`] so it can be faked.

use crate::env::Workspace;
use crate::image_updates::{ImageRef, match_image_line, parse_image_ref};
use crate::render::{overlay_vars, parse_dotenv, referenced_vars, scan_overlay};
use anyhow::{Context as _, Result, bail, ensure};
use rivet_package::{Limits, Manifest, Package, PackageBuilder, PackageError};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The manifest an overlay directory carries, read at pack time and regenerated into the archive.
pub const MANIFEST_FILE: &str = rivet_package::MANIFEST_FILE;

/// Label every installed resource gets, naming the package it came from.
pub const PACKAGE_LABEL: &str = "riveter.forge/package";
/// Annotation recording the exact installed version. An annotation rather than a
/// label because a version such as `0.4.0+b123` is not a legal label value.
pub const VERSION_ANNOTATION: &str = "riveter.forge/package-version";

/// Looks up the digest an image tag points at.
pub trait DigestResolver {
    /// `sha256:<64 hex>` for `image`'s tag.
    fn digest(&self, image: &ImageRef) -> Result<String>;
}

/// What to pack and where to put it.
#[derive(Debug)]
pub struct PackOptions<'a> {
    /// The overlay, which is also the package name.
    pub env: &'a str,
    /// Directory holding `<env>/overlay.yaml`.
    pub overlays_dir: &'a Path,
    /// Where the `.rivet` is written.
    pub out_dir: &'a Path,
    /// Build metadata appended to the manifest's version (`0.4.0` -> `0.4.0+<suffix>`).
    pub version_suffix: Option<&'a str>,
}

/// An `image:` line rewritten to a digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedImage {
    /// Path inside the package.
    pub file: String,
    /// 1-based line.
    pub line: usize,
    /// The reference as written.
    pub from: String,
    /// The reference as packed.
    pub to: String,
}

/// The result of a pack.
#[derive(Debug)]
pub struct Packed {
    /// Where the archive was written.
    pub path: PathBuf,
    /// Its size in bytes.
    pub size_bytes: usize,
    /// Its SHA-256, hex.
    pub sha256: String,
    /// The manifest as packed (suffix and namespace applied).
    pub manifest: Manifest,
    /// Images pinned to a digest.
    pub pinned: Vec<PinnedImage>,
    /// Images left as written, each with the reason.
    pub unpinned: Vec<String>,
    /// Variables the overlay expands that the package's `values.toml` doesn't
    /// default, which the installer must supply.
    pub required_vars: Vec<String>,
    /// Files left out on purpose, so a missing secret is explained rather than mysterious.
    pub excluded: Vec<String>,
}

/// The files of an overlay directory, and the names of those left out.
type Collected = (BTreeMap<String, Vec<u8>>, Vec<String>);

const SCRATCH_MANIFEST_HELP: &str = "an overlay needs a `rivet.toml` next to its `overlay.yaml`:\n\n  \
    [package]\n  name = \"<overlay directory name>\"\n  version = \"0.1.0\"";

/// Builds the package for `opts.env`. With a `resolver`, every image is pinned
/// to its digest; without one they are packed as written.
pub fn pack(opts: &PackOptions<'_>, resolver: Option<&dyn DigestResolver>) -> Result<Packed> {
    let dir = opts.overlays_dir.join(opts.env);
    ensure!(
        dir.join("overlay.yaml").is_file(),
        "overlay not found: {}",
        dir.join("overlay.yaml").display()
    );
    ensure!(
        rivet_package::manifest::is_valid_name(opts.env),
        "`{}` cannot be a package name: it must be a lowercase DNS-1123 label \
         (a-z, 0-9, `-`; at most 63 characters). Rename the overlay directory.",
        opts.env
    );

    let mut manifest = read_manifest(&dir, opts.env)?;
    if let Some(suffix) = opts.version_suffix {
        manifest.package.version = with_build_suffix(&manifest.package.version, suffix)?;
    }

    let (mut files, excluded) = collect_files(&dir)?;

    let scan = scan_overlay(opts.env, opts.overlays_dir)
        .with_context(|| format!("could not render overlay `{}`", opts.env))?;
    check_includes(opts.env, &scan.includes, &files)?;

    if manifest.package.namespace.is_none() {
        manifest.package.namespace = declared_namespace(&scan.rendered);
    }

    let literal_secrets = literal_secret_values(&scan.rendered);
    ensure!(
        literal_secrets.is_empty(),
        "the overlay writes secret value(s) into the package itself:\n  {}\n\n\
         a package is readable by everyone who can read the registry. Reference a variable \
         (`${{NAME}}`) and supply it at install with --env-file or --set",
        literal_secrets.join("\n  ")
    );

    let defaults = match files.get("values.toml") {
        Some(bytes) => {
            parse_values(std::str::from_utf8(bytes).context("values.toml is not UTF-8")?)
                .context("invalid values.toml")?
        }
        None => BTreeMap::new(),
    };
    let required_vars = overlay_vars(&scan.rendered)
        .into_iter()
        .filter(|name| !defaults.contains_key(name))
        .collect();

    let (pinned, unpinned) = match resolver {
        Some(resolver) => pin_images(&mut files, resolver)?,
        None => (Vec::new(), Vec::new()),
    };

    let mut builder = PackageBuilder::new(manifest.clone());
    for (path, bytes) in files {
        builder.add_file(path, bytes)?;
    }
    let bytes = builder.build().map_err(explain)?;

    // Whatever was just written must pass the check a registry will apply.
    Package::read(std::io::Cursor::new(&bytes), &Limits::default())
        .map_err(explain)
        .context("the package just built does not pass its own validation")?;

    std::fs::create_dir_all(opts.out_dir)
        .with_context(|| format!("failed to create {}", opts.out_dir.display()))?;
    ignore_output_dir(opts.out_dir)?;
    let path = opts.out_dir.join(rivet_package::file_name(
        &manifest.package.name,
        &manifest.package.version,
    ));
    std::fs::write(&path, &bytes).with_context(|| format!("failed to write {}", path.display()))?;

    Ok(Packed {
        path,
        size_bytes: bytes.len(),
        sha256: rivet_package::sha256_hex(&bytes),
        manifest,
        pinned,
        unpinned,
        required_vars,
        excluded,
    })
}

/// A package error with the likely fix attached where there is one.
fn explain(err: PackageError) -> anyhow::Error {
    match err {
        PackageError::Missing(file) if file == rivet_package::OVERLAY_FILE => {
            anyhow::anyhow!("the overlay has no `overlay.yaml`")
        }
        other => anyhow::Error::new(other),
    }
}

fn read_manifest(dir: &Path, env: &str) -> Result<Manifest> {
    let path = dir.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read {}: {SCRATCH_MANIFEST_HELP}", path.display()))?;
    let manifest = Manifest::parse(&text).with_context(|| format!("invalid {}", path.display()))?;
    ensure!(
        manifest.package.name == env,
        "{} names the package `{}`, but the overlay directory is `{env}`: they must match, \
         because the overlay's own includes spell the directory name out",
        path.display(),
        manifest.package.name
    );
    Ok(manifest)
}

/// The shortest commit prefix `{sha}` expands to.
const SHA_LEN: usize = 7;

/// Expands `{timestamp}` and `{sha}` in a `--version-suffix`.
///
/// A pipeline step is not run through a shell, so `$(date ...)` is not available to
/// it; these two tokens are what make a useful build suffix from one. `{timestamp}`
/// is the UTC time as `YYYYMMDDHHMMSS`, which sorts chronologically even as text -
/// which is how Warehouse orders two builds of one version. `{sha}` is the first
/// seven characters of `sha`. A suffix with no tokens passes through untouched.
pub fn expand_suffix(
    template: &str,
    now: chrono::DateTime<chrono::Utc>,
    sha: Option<&str>,
) -> Result<String> {
    let mut out = String::new();
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let close = after
            .find('}')
            .with_context(|| format!("unclosed `{{` in --version-suffix `{template}`"))?;

        match &after[..close] {
            "timestamp" => out.push_str(&now.format("%Y%m%d%H%M%S").to_string()),
            "sha" => {
                let sha = sha.filter(|s| !s.is_empty()).context(
                    "--version-suffix asks for the commit, but none is known: set CONVEYOR_SHA \
                     (conveyor does) or GITHUB_SHA",
                )?;
                out.push_str(&sha.chars().take(SHA_LEN).collect::<String>());
            }
            other => bail!(
                "unknown token `{{{other}}}` in --version-suffix (known: {{timestamp}}, {{sha}})"
            ),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// `version` with `+<suffix>` appended, refusing a version that already has build metadata.
pub fn with_build_suffix(version: &str, suffix: &str) -> Result<String> {
    let parsed = semver::Version::parse(version)
        .with_context(|| format!("version `{version}` is not semver"))?;
    ensure!(
        parsed.build.is_empty(),
        "version `{version}` already has build metadata; drop it from rivet.toml and let \
         --version-suffix add it"
    );
    ensure!(!suffix.is_empty(), "--version-suffix cannot be empty");
    semver::BuildMetadata::new(suffix).with_context(|| {
        format!("`{suffix}` is not valid build metadata (dot-separated [0-9A-Za-z-])")
    })?;
    Ok(format!("{version}+{suffix}"))
}

/// Regular files of the overlay directory, keyed by `/`-separated relative
/// path, and the names of those deliberately left out.
///
/// Left out: dotfiles (so `.env`, which holds secrets, can never be packed -
/// `.env.example` is the one exception, being its documentation), the manifest
/// and checksum file, which the builder generates, and anything inside a hidden
/// directory. A symlink is an error rather than a skip: following it could
/// pull in a file from anywhere, and silently dropping it would ship a package
/// that renders differently from the overlay it came from.
fn collect_files(dir: &Path) -> Result<Collected> {
    let mut files = BTreeMap::new();
    let mut excluded = Vec::new();

    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0 || entry.file_type().is_file() || !is_hidden(entry.file_name())
        });

    for entry in walker {
        let entry = entry?;
        let relative = entry
            .path()
            .strip_prefix(dir)
            .context("walked outside the overlay")?;
        if entry.depth() == 0 || entry.file_type().is_dir() {
            continue;
        }
        let name = relative
            .components()
            .map(|c| {
                c.as_os_str()
                    .to_str()
                    .map(str::to_string)
                    .with_context(|| format!("{} is not valid UTF-8", relative.display()))
            })
            .collect::<Result<Vec<_>>>()?
            .join("/");

        ensure!(
            !entry.file_type().is_symlink(),
            "{name} is a symlink; replace it with the file itself before packing"
        );

        let hidden = is_hidden(entry.file_name()) && name != ".env.example";
        let generated = name == MANIFEST_FILE || name == rivet_package::SUMS_FILE;
        if hidden || generated {
            if hidden {
                excluded.push(name);
            }
            continue;
        }

        let bytes = std::fs::read(entry.path())
            .with_context(|| format!("failed to read {}", entry.path().display()))?;
        files.insert(name, bytes);
    }

    ensure!(
        files.contains_key(rivet_package::OVERLAY_FILE),
        "the overlay has no overlay.yaml"
    );
    Ok((files, excluded))
}

fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|n| n.starts_with('.'))
}

/// Every template the overlay loaded must live inside its own directory - the
/// package only holds that directory - and must be among the files packed.
fn check_includes(env: &str, includes: &[String], files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let prefix = format!("{env}/");
    let mut outside = Vec::new();
    let mut unpackable = Vec::new();

    for include in includes {
        match include.strip_prefix(&prefix) {
            None => outside.push(include.as_str()),
            Some(inner) if !files.contains_key(inner) => unpackable.push(include.as_str()),
            Some(_) => {}
        }
    }

    ensure!(
        outside.is_empty(),
        "the overlay includes template(s) outside `{env}/`, which a package cannot carry:\n  {}\n\n\
         a package is one overlay directory; move what is shared into it",
        outside.join("\n  ")
    );
    ensure!(
        unpackable.is_empty(),
        "the overlay includes template(s) that cannot be packed (hidden file?):\n  {}",
        unpackable.join("\n  ")
    );
    Ok(())
}

/// Secret values in a rendered overlay that are not a `${VAR}` placeholder, as `kind/name: key`.
///
/// A package is readable by anyone who can read the registry, a wider audience than the
/// private repository it was built from, so a credential written into a Secret resource must
/// never travel in it. A placeholder is the way to keep a Secret in an overlay at all: the value
/// arrives at install. A `raw` resource cannot be inspected this way, so one that declares a
/// Secret is refused outright. The check reads the overlay before `${VAR}` expansion - after it,
/// a placeholder would be indistinguishable from a literal.
#[must_use]
pub fn literal_secret_values(rendered: &str) -> Vec<String> {
    let Ok(overlay) = serde_yaml::from_str::<serde_yaml::Value>(rendered) else {
        return Vec::new();
    };
    let Some(resources) = overlay
        .get("resources")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for resource in resources {
        let kind = resource
            .get("kind")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or_default();
        let name = resource
            .get("name")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("?");

        if kind.eq_ignore_ascii_case("raw") {
            let text = serde_yaml::to_string(resource).unwrap_or_default();
            if text
                .lines()
                .any(|line| line.trim().trim_start_matches("- ") == "kind: Secret")
            {
                found.push(format!(
                    "raw/{name}: declares a Secret, which cannot be checked"
                ));
            }
            continue;
        }
        if !kind.eq_ignore_ascii_case("secret") {
            continue;
        }

        for field in ["data", "string_data", "stringData"] {
            let Some(values) = resource.get(field).and_then(serde_yaml::Value::as_mapping) else {
                continue;
            };
            for (key, value) in values {
                let placeholder = value
                    .as_str()
                    .is_some_and(|text| !referenced_vars(text).is_empty());
                if !placeholder {
                    let key = key.as_str().unwrap_or("?");
                    found.push(format!("secret/{name}: {key}"));
                }
            }
        }
    }
    found
}

/// The overlay's `namespace_name`, unless it is itself a `${VAR}` that only an install can resolve.
fn declared_namespace(rendered: &str) -> Option<String> {
    let value: serde_yaml::Value = serde_yaml::from_str(rendered).ok()?;
    let namespace = value.get("namespace_name")?.as_str()?.trim();
    (rivet_package::manifest::is_valid_name(namespace)).then(|| namespace.to_string())
}

/// `values.toml`: a flat table of defaults for the overlay's `${VAR}`s.
pub fn parse_values(text: &str) -> Result<BTreeMap<String, String>> {
    let table: toml::Table = toml::from_str(text)?;
    let mut values = BTreeMap::new();

    for (key, value) in table {
        ensure!(
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'),
            "`{key}` is not a usable variable name"
        );
        let text = match value {
            toml::Value::String(s) => s,
            toml::Value::Integer(i) => i.to_string(),
            toml::Value::Float(f) => f.to_string(),
            toml::Value::Boolean(b) => b.to_string(),
            _ => bail!("`{key}` must be a string, number or boolean"),
        };
        values.insert(key, text);
    }

    Ok(values)
}

/// Rewrites every `image:` line that names a tag to also name that tag's digest
/// (`repo:tag@sha256:...`), keeping the tag for readability. Lines already
/// pinned are left alone; ones that cannot be resolved to a reference - a
/// `${VAR}` in the image, no tag - are reported rather than guessed at.
fn pin_images(
    files: &mut BTreeMap<String, Vec<u8>>,
    resolver: &dyn DigestResolver,
) -> Result<(Vec<PinnedImage>, Vec<String>)> {
    let mut pinned = Vec::new();
    let mut unpinned = Vec::new();
    let mut digests: HashMap<String, String> = HashMap::new();

    for (path, bytes) in files.iter_mut() {
        if !(path.ends_with(".yaml.j2") || path == rivet_package::OVERLAY_FILE) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };

        let mut rewritten = String::with_capacity(text.len() + 128);
        let mut changed = false;

        for (index, segment) in text.split_inclusive('\n').enumerate() {
            let line = segment.trim_end_matches(['\n', '\r']);
            let ending = &segment[line.len()..];

            let Some((indent, value)) = match_image_line(line) else {
                rewritten.push_str(segment);
                continue;
            };

            if value.contains('@') {
                rewritten.push_str(segment);
                continue;
            }
            if value.contains('$') || value.contains("{{") || value.contains("{%") {
                unpinned.push(format!(
                    "{path}:{}: {value} (image is templated)",
                    index + 1
                ));
                rewritten.push_str(segment);
                continue;
            }
            let image = match parse_image_ref(value) {
                Ok(image) => image,
                Err(reason) => {
                    unpinned.push(format!("{path}:{}: {value} ({reason})", index + 1));
                    rewritten.push_str(segment);
                    continue;
                }
            };

            let digest = if let Some(known) = digests.get(value) {
                known.clone()
            } else {
                let digest = resolver.digest(&image).with_context(|| {
                    format!(
                        "could not pin {path}:{} to a digest; packing needs access to the \
                         registry (pass --no-pin to pack without)",
                        index + 1
                    )
                })?;
                ensure!(
                    crate::image_updates::is_sha256_digest(&digest),
                    "registry returned `{digest}` for {value}, which is not a sha256 digest"
                );
                digests.insert(value.to_string(), digest.clone());
                digest
            };

            let to = format!("{value}@{digest}");
            let _ = write!(rewritten, "{indent}image: {to}{ending}");
            pinned.push(PinnedImage {
                file: path.clone(),
                line: index + 1,
                from: value.to_string(),
                to,
            });
            changed = true;
        }

        if changed {
            *bytes = rewritten.into_bytes();
        }
    }

    Ok((pinned, unpinned))
}

/// Keeps `.rivet` files out of version control, like the rendered `manifests/`.
fn ignore_output_dir(dir: &Path) -> Result<()> {
    let path = dir.join(".gitignore");
    if path.exists() {
        return Ok(());
    }
    std::fs::write(
        &path,
        "# Written by riveter: packages are build output.\n*.rivet\n",
    )
    .with_context(|| format!("failed to write {}", path.display()))
}

// ---------------------------------------------------------------------------
// Install
// ---------------------------------------------------------------------------

/// A package unpacked into a scratch tree, ready to render and apply. Dropping
/// it deletes the tree.
#[derive(Debug)]
pub struct Installation {
    _scratch: tempfile::TempDir,
    /// The overlay name, which is the package name.
    pub env: String,
    /// What to hand to [`crate::env::with_workspace`] around the render and apply.
    pub workspace: Workspace,
}

/// Where an install's variables come from, lowest precedence first.
#[derive(Debug, Default)]
pub struct InstallValues<'a> {
    /// A dotenv-format file, typically the overlay's own `.env`.
    pub env_file: Option<&'a Path>,
    /// `KEY=value` overrides.
    pub sets: &'a [String],
}

/// The variables an install renders with.
///
/// The package's `values.toml` defaults, then `env_file`, then `sets`. The
/// cwd's `.env` is deliberately not consulted - an install should be a function
/// of the package and what was passed, not of whichever directory it ran from.
pub fn merge_values(
    package: &Package,
    values: &InstallValues<'_>,
) -> Result<HashMap<String, String>> {
    let mut merged: HashMap<String, String> = match package.files.get("values.toml") {
        Some(bytes) => {
            parse_values(std::str::from_utf8(bytes).context("values.toml is not UTF-8")?)
                .context("invalid values.toml in the package")?
                .into_iter()
                .collect()
        }
        None => HashMap::new(),
    };

    if let Some(path) = values.env_file {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        merged.extend(parse_dotenv(&text));
    }

    for set in values.sets {
        let (key, value) = set
            .split_once('=')
            .filter(|(key, _)| !key.trim().is_empty())
            .with_context(|| format!("--set expects KEY=value, got `{set}`"))?;
        merged.insert(key.trim().to_string(), value.to_string());
    }

    Ok(merged)
}

/// Checks the manifest's `requires.riveter` against this binary, returning
/// notes for anything that could not be checked.
pub fn check_requirements(manifest: &Manifest, riveter_version: &str) -> Result<Vec<String>> {
    let mut notes = Vec::new();

    if let Some(requirement) = &manifest.requires.riveter {
        let required = semver::VersionReq::parse(requirement)?;
        let running = semver::Version::parse(riveter_version)?;
        ensure!(
            required.matches(&running),
            "{} {} requires riveter {requirement}, but this is riveter {riveter_version}",
            manifest.package.name,
            manifest.package.version
        );
    }

    for entry in &manifest.requires.packages {
        notes.push(format!(
            "requires package `{entry}` - not verified; riveter does not yet check what else is installed"
        ));
    }

    Ok(notes)
}

/// Unpacks `package` into a scratch tree laid out as riveter expects
/// (`overlays/<name>/...`) and builds the workspace that points a render at it.
pub fn prepare_install<S: std::hash::BuildHasher>(
    package: &Package,
    vars: HashMap<String, String, S>,
) -> Result<Installation> {
    let scratch = tempfile::tempdir().context("failed to create a scratch directory")?;
    let env = package.manifest.package.name.clone();

    let overlays_root = scratch.path().join("overlays");
    let unpacked = overlays_root.join(&env);
    std::fs::create_dir_all(&unpacked)?;
    package.write_to(&unpacked)?;

    let workspace = Workspace {
        overlays_dir: Some(overlays_root),
        output_dir: Some(scratch.path().join("manifests")),
        env_vars: Some(vars.into_iter().collect()),
        resource_labels: BTreeMap::from([(PACKAGE_LABEL.to_string(), env.clone())]),
        resource_annotations: BTreeMap::from([(
            VERSION_ANNOTATION.to_string(),
            package.manifest.package.version.clone(),
        )]),
    };

    Ok(Installation {
        _scratch: scratch,
        env,
        workspace,
    })
}

/// Reads a package from a file, applying the registry's own checks.
pub fn read_file(path: &Path) -> Result<Package> {
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    Package::read(file, &Limits::default())
        .map_err(explain)
        .with_context(|| format!("{} is not a valid package", path.display()))
}
