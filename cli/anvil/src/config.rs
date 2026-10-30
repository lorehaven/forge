use quench_cli::prelude::{Tone, print_status};
use quench_core::Result;
use quench_core::{Context, bail};
use quench_serde::{Deserialize, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub docker: DockerConfig,

    #[serde(default)]
    pub install: InstallConfig,

    #[serde(default)]
    pub release: ReleaseConfig,

    #[serde(default)]
    pub artifacts: ArtifactsConfig,

    #[serde(default)]
    pub docs: DocsConfig,
}

/// `anvil docs`: documentation books that are checked, built with Jekyll and sent to the warehouse.
#[derive(Debug, Default, Deserialize)]
pub struct DocsConfig {
    /// Where the builds go, from the workspace root (default `target/docs`).
    #[serde(default)]
    pub out: Option<String>,
    /// The command that runs Jekyll, split on spaces (default `jekyll`; `bundle exec jekyll` for a Gemfile).
    #[serde(default)]
    pub jekyll: Option<String>,
    /// The warehouse, including its base path; `[artifacts].warehouse_url` when empty.
    #[serde(default)]
    pub warehouse_url: Option<String>,
    /// The warehouse files storage the books are kept in (`docs`): required to release.
    #[serde(default)]
    pub storage: Option<String>,
    /// The books, by name: `[docs.books.<name>]`.
    #[serde(default)]
    pub books: BTreeMap<String, DocsBook>,
}

/// One Jekyll site made of Markdown files.
#[derive(Debug, Default, Deserialize)]
pub struct DocsBook {
    /// The site's source directory, from the workspace root.
    pub dir: String,
    /// The name shown with the book (default: the book's name).
    #[serde(default)]
    pub title: Option<String>,
    /// The version of the book, literally. Without it: the version of `version_file`, else today's date.
    #[serde(default)]
    pub version: Option<String>,
    /// A `Cargo.toml` (from the workspace root) whose version is the book's.
    #[serde(default)]
    pub version_file: Option<String>,
    /// The `baseurl` the site is built for (`/docs/forge`): empty for a site at the root.
    #[serde(default)]
    pub baseurl: Option<String>,
    /// Checks each `openapi.yaml` under `<dir>/<specs>/<name>/` against the routes in the source.
    #[serde(default)]
    pub openapi: Option<DocsOpenApi>,
}

/// Where a book's `OpenAPI` documents are and where the routes they describe live.
#[derive(Debug, Default, Deserialize)]
pub struct DocsOpenApi {
    /// The directory of the book that holds one directory per service (`services`).
    pub specs: String,
    /// The service's source directories, from the workspace root; `{name}` is the service (`services/{name}/service/src`).
    pub sources: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DockerConfig {
    #[serde(default)]
    pub registry: String,
    /// Name of a private cargo registry the Dockerfile build needs (e.g. one
    /// declared in `~/.cargo/config.toml`'s `[registries.<name>]`). Anvil
    /// forwards it as `CARGO_REGISTRIES_<NAME>_INDEX`/`_TOKEN` build-args
    /// when set; unset means the build has no private registry to see.
    #[serde(default)]
    pub cargo_registry: Option<String>,
    /// Optional literal override for that registry's index URL. Falls back
    /// to the `CARGO_REGISTRIES_<NAME>_INDEX` env var, then to the host's own
    /// `~/.cargo/config.toml`, if unset.
    #[serde(default)]
    pub cargo_registry_index: Option<String>,
    /// Mapping from module name to Docker module configuration
    #[serde(default)]
    pub modules: HashMap<String, DockerModuleConfig>,
}

#[derive(Debug, Deserialize)]
pub struct DockerModuleConfig {
    pub packages: Vec<String>,
    pub dockerfile: String,

    #[serde(default, flatten)]
    pub package_overrides: HashMap<String, DockerPackageOverride>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DockerPackageOverride {
    #[serde(default)]
    pub dockerfile: Option<String>,

    #[serde(default)]
    pub module_name: Option<String>,

    #[serde(default)]
    pub image_name: Option<String>,

    #[serde(default)]
    pub registries: Vec<String>,

    // Deprecated single-registry override, kept for backward compatibility.
    #[serde(default)]
    pub registry: Option<String>,

    /// Extra `--build-arg` values for this package.
    ///
    /// What lets one Dockerfile serve every service: a package that needs an
    /// extra runtime package, a different resource directory or its own user
    /// says so here, rather than forking the whole file over a line of it.
    ///
    /// A `BTreeMap` rather than a `HashMap` so the arguments reach `docker
    /// build` in the same order every time - a build that varies by iteration
    /// order is one whose cache misses at random.
    #[serde(default)]
    pub build_args: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct InstallConfig {
    /// List of packages to install
    #[serde(default)]
    pub packages: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ReleaseConfig {
    /// Cargo registry to publish to
    #[serde(default)]
    pub registry: String,
    /// List of releasable packages
    #[serde(default)]
    pub packages: Vec<String>,
    /// Template for the version-bump commit message.
    ///
    /// See `commands::release::render_commit_message` for the placeholders
    /// it supports and `commands::release::DEFAULT_COMMIT_MESSAGE_TEMPLATE`
    /// for what an unset one falls back to.
    #[serde(default)]
    pub commit_message_template: Option<String>,
    /// Gatehouse (including its base path) that `anvil login` mints application
    /// tokens at. Not a secret, so it lives here rather than in the secret file.
    #[serde(default)]
    pub gatehouse_url: Option<String>,
}

/// What `anvil artifact release` sends to the warehouse: apps and desktop builds, not crates or images.
#[derive(Debug, Default, Deserialize)]
pub struct ArtifactsConfig {
    /// The warehouse, including its base path (`https://host/warehouse`).
    #[serde(default)]
    pub warehouse_url: Option<String>,
    /// What can be released, by name: `[artifacts.programs.<name>]`.
    #[serde(default)]
    pub programs: BTreeMap<String, ArtifactProgram>,
}

/// One program and the builds it ships as.
#[derive(Debug, Default, Deserialize)]
pub struct ArtifactProgram {
    /// The application id the warehouse files every platform's build under (`dev.lorehaven.hoist`).
    pub program: String,
    /// Where the builds run and `file` is looked for, from the workspace root (default: the root).
    #[serde(default)]
    pub dir: Option<String>,
    /// The `Cargo.toml` whose version is the release's (`[workspace.package]`, else `[package]`), from `dir`.
    pub version_file: String,
    #[serde(default)]
    pub targets: Vec<ArtifactTarget>,
    /// The program's service image, for a project that is not a member of this workspace (so `anvil release` cannot
    /// see it). Built, tagged and pushed with the same version; `--target image` releases only it.
    #[serde(default)]
    pub image: Option<ArtifactImage>,
}

/// A docker image released with a program.
#[derive(Debug, Default, Deserialize)]
pub struct ArtifactImage {
    /// The Dockerfile, from the program's `dir`.
    pub dockerfile: String,
    /// The build context, from `dir` (default: `dir` itself).
    #[serde(default)]
    pub context: Option<String>,
    /// The tag's middle part: `<registry>/<module>/<name>:<version>`.
    pub module: String,
    pub name: String,
    /// Where it is pushed; `[docker].registry` when empty.
    #[serde(default)]
    pub registries: Vec<String>,
    /// Extra `--build-arg` values.
    #[serde(default)]
    pub build_args: BTreeMap<String, String>,
}

/// One build of the program for one platform.
#[derive(Debug, Default, Deserialize)]
pub struct ArtifactTarget {
    /// What `--target` calls it (default: the platform).
    #[serde(default)]
    pub name: Option<String>,
    /// `android`, `linux`, `windows` or `macos`.
    pub platform: String,
    /// `apk` for Android (the warehouse reads the manifest); required for everything else (`deb`, `tar.gz`, ...).
    #[serde(default)]
    pub format: Option<String>,
    /// `x86_64`, `aarch64` or `universal`.
    #[serde(default)]
    pub arch: Option<String>,
    /// What people see for this build, if not the program's own label.
    #[serde(default)]
    pub label: Option<String>,
    /// The command that makes the file, run in `dir`. `${version_name}`, `${version_code}` and `${program}` are
    /// replaced in every argument, and the first two are also in the environment as `ANVIL_VERSION_NAME` and
    /// `ANVIL_VERSION_CODE`.
    pub build: Vec<String>,
    /// The file the build leaves, from `dir`; the same placeholders are replaced in it.
    pub file: String,
}

/// The configuration file at the workspace root.
pub const FILE: &str = ".anvil.toml";

/// The directory that may hold the configuration instead, or besides: every `*.toml` in it is a part of it.
pub const DIR: &str = ".anvil";

/// What a tool's credentials are called inside [`DIR`]. Never read as configuration (it holds a token, and its keys are
/// not anvil's).
pub const SECRET: &str = "secret.toml";

/// The files the configuration is made of, in the order they are read: `.anvil.toml`, then `.anvil/*.toml` by name
/// (not [`SECRET`], and not what is in a directory below).
#[must_use]
pub fn files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let single = root.join(FILE);
    if single.is_file() {
        found.push(single);
    }
    if let Ok(entries) = fs::read_dir(root.join(DIR)) {
        let mut parts: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "toml"))
            .filter(|p| p.file_name().is_some_and(|n| n != SECRET))
            .collect();
        parts.sort();
        found.extend(parts);
    }
    found
}

/// The configuration as one table, and the files it came from.
#[derive(Debug)]
pub struct Merged {
    pub value: Value,
    pub files: Vec<PathBuf>,
}

/// Reads every file of the configuration under `root` and merges them.
///
/// A table in two files is one table with the keys of both; any other value (a string, a number, an array) set in two
/// files is an error naming both, because which should win is nobody's guess.
///
/// # Errors
/// There is no configuration file, one cannot be read or is not TOML, or two files set the same value.
pub fn merge(root: &Path) -> quench_core::Result<Merged> {
    let files = files(root);
    if files.is_empty() {
        bail!("no {FILE} and no {DIR}/*.toml in {}", root.display());
    }
    let mut table = Map::new();
    let mut owners: BTreeMap<String, PathBuf> = BTreeMap::new();
    for file in &files {
        let text =
            fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        let Value::Object(part) = quench_serde::toml::parse(&text)
            .with_context(|| format!("{} is not valid TOML", file.display()))?
        else {
            bail!("{} is not a TOML table", file.display());
        };
        merge_table(&mut table, &part, "", file, &mut owners)?;
    }
    Ok(Merged {
        value: Value::Object(table),
        files,
    })
}

/// Notes `file` as where `path` and every key below it came from.
fn record(owners: &mut BTreeMap<String, PathBuf>, path: &str, value: &Value, file: &Path) {
    owners.insert(path.to_string(), file.to_path_buf());
    if let Value::Object(table) = value {
        for (key, inner) in table.iter() {
            record(owners, &format!("{path}.{key}"), inner, file);
        }
    }
}

fn merge_table(
    into: &mut Map,
    from: &Map,
    path: &str,
    file: &Path,
    owners: &mut BTreeMap<String, PathBuf>,
) -> quench_core::Result<()> {
    for (key, value) in from.iter() {
        let here = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        match (into.get_mut(key), value) {
            (None, _) => {
                into.insert(key.clone(), value.clone());
                record(owners, &here, value, file);
            }
            (Some(Value::Object(existing)), Value::Object(more)) => {
                merge_table(existing, more, &here, file, owners)?;
            }
            (Some(_), _) => {
                let first = owners.get(&here).map_or_else(
                    || "an earlier file".to_string(),
                    |p| p.display().to_string(),
                );
                bail!("`{here}` is set in both {first} and {}", file.display());
            }
        }
    }
    Ok(())
}

/// The configuration under `root`, strictly: anything wrong is an error (what `anvil config check` and the tests use).
///
/// # Errors
/// As [`merge`], or the merged tables do not fit anvil's configuration.
pub fn load_from(root: &Path) -> quench_core::Result<Config> {
    let merged = merge(root)?;
    quench_serde::from_value(&merged.value).context("the configuration is not valid")
}

/// `quiet` suppresses the fallback warnings below.
///
/// Set it for a `--json` command, whose stdout is meant to be piped straight
/// to a consumer and must carry nothing but the requested structured report
/// (`print_status` prints to stdout, same as anvil's other status chatter).
pub fn load_config(quiet: bool) -> Result<Config> {
    let warn = |message: &str| {
        if !quiet {
            print_status(Tone::Warn, "anvil", message);
        }
    };

    let config = match merge(Path::new(".")) {
        Err(err) => {
            warn(&format!("{err}, defaulting to empty config"));
            Config::default()
        }
        Ok(merged) => quench_serde::from_value(&merged.value).unwrap_or_else(|err| {
            warn(&format!(
                "failed to read the configuration ({err}), defaulting to empty config"
            ));
            Config::default()
        }),
    };

    Ok(config)
}
