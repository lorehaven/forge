use quench_cli::prelude::{Tone, print_status};
use quench_core::Result;
use quench_serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::fs;

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

    let config = fs::read_to_string(".anvil.toml").map_or_else(
        |_| {
            warn("failed to read .anvil.toml, defaulting to empty config");
            Config::default()
        },
        |content| {
            quench_serde::toml::from_str(&content).unwrap_or_else(|err| {
                warn(&format!(
                    "failed to parse .anvil.toml ({err}), defaulting to empty config"
                ));
                Config::default()
            })
        },
    );

    Ok(config)
}
