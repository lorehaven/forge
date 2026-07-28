//! The shape of `foreman.toml`.
//!
//! Every string in here is a template: `${name}` is replaced from `[vars]` plus
//! whatever the surrounding context contributes (a service lends its `name`,
//! `package`, `port` and `base_path`; a warning lends the `pids` it found). See
//! `vars.rs` for the substitution itself.
//!
//! Unknown keys are a hard error rather than a shrug. A misspelled key in a
//! config file is a setting that silently does nothing, which is the kind of
//! bug that costs an afternoon.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Searched for in the working directory and every parent, in this order.
pub const CONFIG_NAMES: [&str; 2] = ["foreman.toml", ".foreman.toml"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub project: Project,
    #[serde(default)]
    pub vars: BTreeMap<String, Var>,
    #[serde(default)]
    pub build: Build,
    #[serde(default)]
    pub containers: Vec<Container>,
    #[serde(default)]
    pub tasks: BTreeMap<String, Task>,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub services: Vec<Service>,
    #[serde(default)]
    pub notes: Vec<Note>,
    #[serde(default)]
    pub warnings: Vec<Warning>,
    pub test: Option<Test>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    #[serde(default = "default_project_name")]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Where pid files and logs live, relative to the project root.
    #[serde(default = "default_run_dir")]
    pub run_dir: String,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: default_project_name(),
            description: String::new(),
            run_dir: default_run_dir(),
        }
    }
}

/// A `[vars]` entry: either a literal, or a value lifted out of a dotenv file.
///
/// Reading from the service's own `.env` rather than restating the value here
/// is what keeps a shared secret shared: the config cannot quietly drift from
/// what the services already expect.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Var {
    Literal(String),
    EnvFile {
        /// Path to the dotenv file, relative to the project root.
        env_file: String,
        key: String,
        #[serde(default)]
        default: String,
    },
}

/// How a service's binary is produced and where it lands.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    #[serde(default = "default_build_command")]
    pub command: Vec<String>,
    #[serde(default = "default_binary")]
    pub binary: String,
}

impl Default for Build {
    fn default() -> Self {
        Self {
            command: default_build_command(),
            binary: default_binary(),
        }
    }
}

/// A docker container the estate sits on. Started before any service, stopped
/// only by `foreman stop all`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Container {
    pub name: String,
    pub image: String,
    /// Defaults to `name`. Set it when the container on the host is called
    /// something other than the name you want to type.
    pub container_name: Option<String>,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Extra `docker run` arguments, inserted before the image.
    #[serde(default)]
    pub args: Vec<String>,
    /// Run inside the container until it succeeds; empty means don't wait.
    #[serde(default)]
    pub ready: Vec<String>,
    #[serde(default = "default_container_timeout")]
    pub ready_timeout_secs: u64,
    /// Only for the message; `localhost:5432` and the like.
    pub address: Option<String>,
}

impl Container {
    pub fn container_name(&self) -> &str {
        self.container_name.as_deref().unwrap_or(&self.name)
    }
}

/// What a task is for. `migrate` and `reset` get their own verbs because the
/// database is the one thing every service in an estate shares; anything else
/// is `foreman run <name>`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// Runs before services start, and on `foreman db`.
    Migrate,
    /// Runs on `foreman reset`.
    Reset,
    #[default]
    Manual,
}

/// When to expand `each_selected` into the command line.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SelectionMode {
    /// Only when a strict subset of the estate was asked for. Starting
    /// everything means the task's own configuration already covers it.
    #[default]
    SubsetOnly,
    Always,
    Never,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    #[serde(default)]
    pub role: Role,
    pub description: Option<String>,
    /// Containers that have to be up before this runs.
    #[serde(default)]
    pub containers: Vec<String>,
    /// Run first, and abort the task if it fails. Typically a cargo build.
    #[serde(default)]
    pub build: Vec<String>,
    pub command: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Relative to the project root; defaults to the project root itself.
    pub workdir: Option<String>,
    /// Repeated once per selected service, in place of the literal
    /// `${selection}` element of `command` (or appended, if there isn't one).
    #[serde(default)]
    pub each_selected: Vec<String>,
    #[serde(default)]
    pub selection: SelectionMode,
    /// Take the services down first. What drops schemas cannot run under them.
    #[serde(default)]
    pub stop_services: bool,
    /// Printed as a warning before the command runs.
    pub warn: Option<String>,
    /// Printed on success.
    pub done: Option<String>,
}

/// Settings shared by every service. A service's own value wins; `env` is
/// merged key by key rather than replaced, so a service adds to the shared
/// environment instead of restating it.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    pub scheme: Option<String>,
    pub host: Option<String>,
    pub health_path: Option<String>,
    pub start_timeout_secs: Option<u64>,
    pub stop_timeout_secs: Option<u64>,
    /// Relative to the project root. `${package}` and `${name}` are available.
    pub workdir: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub cert_files: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub name: String,
    /// The cargo package, and by default the name of both the binary and the
    /// working directory.
    pub package: String,
    pub port: u16,
    #[serde(default)]
    pub base_path: String,
    /// What this service cannot start without. Selecting it selects these too,
    /// and the order services are listed in the file is the order they start.
    #[serde(default)]
    pub needs: Vec<String>,

    pub scheme: Option<String>,
    pub host: Option<String>,
    pub health_path: Option<String>,
    pub start_timeout_secs: Option<u64>,
    pub stop_timeout_secs: Option<u64>,
    pub workdir: Option<String>,
    /// Overrides `[build].binary` for this service.
    pub binary: Option<String>,
    /// Overrides `[build].command` for this service.
    pub build: Option<Vec<String>>,

    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Dropped from the environment, including anything inherited from the
    /// shell. Use it to keep a shared default off one service.
    #[serde(default)]
    pub unset: Vec<String>,
    #[serde(default)]
    pub env_when: Vec<EnvWhen>,

    /// Borrow another directory's dev certificate when this service has none.
    /// Relative to the project root.
    pub cert_from: Option<String>,
    #[serde(default)]
    pub cert_files: Vec<String>,

    /// Run before this service is signalled, and only when it is up. For
    /// children the service owns rather than we do.
    #[serde(default)]
    pub pre_stop: Vec<Hook>,
}

/// Environment applied only when the condition holds.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvWhen {
    /// Applies when this variable is set in foreman's own environment.
    pub env_set: String,
    pub env: BTreeMap<String, String>,
    /// Printed when it applies, so an unusual startup says so.
    pub note: Option<String>,
}

/// A shell command run at a particular moment. `sh -c`, so a pipeline or a
/// loop is fine - which is what talking to a service's API usually takes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    pub description: Option<String>,
    pub shell: String,
    #[serde(default = "default_hook_timeout")]
    pub timeout_secs: u64,
    /// Wait this long after the hook before signalling. Children do not always
    /// go down with the request.
    #[serde(default)]
    pub settle_secs: u64,
}

#[derive(Debug, Default, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToneName {
    #[default]
    Info,
    Ok,
    Warn,
    Error,
}

/// Printed after a successful start.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    #[serde(default)]
    pub tone: ToneName,
    pub label: String,
    pub message: String,
    /// Only printed when this service is part of what started.
    pub when_selected: Option<String>,
}

/// Processes that can outlive the service that spawned them. Reported after a
/// stop rather than killed: some of them may not be ours.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Warning {
    pub name: String,
    /// Passed to `pgrep -f`.
    pub pgrep: String,
    /// `${pids}` is the space-separated list that matched.
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Test {
    pub command: Vec<String>,
    /// How a bare service name on the command line reaches the suite.
    #[serde(default)]
    pub service_arg: Vec<String>,
    /// The suite usually binds the same ports as the estate, so the two cannot
    /// be up at once.
    #[serde(default = "default_true")]
    pub stop_services: bool,
    pub note: Option<String>,
}

impl Config {
    /// Walks up from `start` looking for a config file. The directory holding
    /// it is the project root, which is what every relative path in the file is
    /// relative to - services run from their own working directory, so a path
    /// that is relative to anything else will not survive the trip.
    pub fn discover(start: &Path) -> Result<(PathBuf, PathBuf)> {
        let mut dir = Some(start);
        while let Some(current) = dir {
            for name in CONFIG_NAMES {
                let candidate = current.join(name);
                if candidate.is_file() {
                    return Ok((current.to_path_buf(), candidate));
                }
            }
            dir = current.parent();
        }
        bail!(
            "no {} found in {} or any parent directory (`foreman init` writes one)",
            CONFIG_NAMES.join(" or "),
            start.display()
        )
    }

    pub fn parse(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let config: Config =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    /// Catches the mistakes that would otherwise surface as a confusing failure
    /// halfway through a start: a dependency on a service that is not in the
    /// table, a task pointing at a container that does not exist.
    fn validate(&self) -> Result<()> {
        let names: Vec<&str> = self.services.iter().map(|s| s.name.as_str()).collect();

        for service in &self.services {
            if names.iter().filter(|n| **n == service.name).count() > 1 {
                bail!("service '{}' is defined more than once", service.name);
            }
            for need in &service.needs {
                if !names.contains(&need.as_str()) {
                    bail!(
                        "service '{}' needs '{}', which is not a configured service",
                        service.name,
                        need
                    );
                }
            }
        }

        let containers: Vec<&str> = self.containers.iter().map(|c| c.name.as_str()).collect();
        for (name, task) in &self.tasks {
            if task.command.is_empty() {
                bail!("task '{name}' has an empty command");
            }
            for container in &task.containers {
                if !containers.contains(&container.as_str()) {
                    bail!("task '{name}' wants container '{container}', which is not configured");
                }
            }
        }

        for note in &self.notes {
            if let Some(name) = &note.when_selected
                && !names.contains(&name.as_str())
            {
                bail!("note is conditional on '{name}', which is not a configured service");
            }
        }

        Ok(())
    }

    pub fn service(&self, name: &str) -> Option<&Service> {
        self.services.iter().find(|s| s.name == name)
    }

    pub fn container(&self, name: &str) -> Option<&Container> {
        self.containers.iter().find(|c| c.name == name)
    }

    /// Tasks in this role, in the order the file lists them.
    pub fn tasks_with_role(&self, role: Role) -> Vec<(&str, &Task)> {
        self.tasks
            .iter()
            .filter(|(_, task)| task.role == role)
            .map(|(name, task)| (name.as_str(), task))
            .collect()
    }
}

fn default_project_name() -> String {
    "project".to_string()
}

fn default_run_dir() -> String {
    ".run".to_string()
}

fn default_build_command() -> Vec<String> {
    ["cargo", "build", "-q", "--package", "${package}"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn default_binary() -> String {
    "target/debug/${package}".to_string()
}

fn default_container_timeout() -> u64 {
    30
}

fn default_hook_timeout() -> u64 {
    30
}

fn default_true() -> bool {
    true
}
