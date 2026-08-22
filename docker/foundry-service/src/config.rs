//! Configuration for a run: what to install, where, and against which ledger.
//!
//! Precedence is CLI flags, then environment, then the config file, then
//! defaults - so a Job manifest can override the baked-in `config/install.toml`
//! with plain env vars.

use anyhow::{Context, Result, bail};
use quench_db::prelude::InstallRequest;
use semver::Version;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const DEFAULT_CATALOG: &str = "migrations";
pub const DEFAULT_CONFIG: &str = "config/install.toml";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub catalog: Option<PathBuf>,
    pub database_url: Option<String>,
    pub ledger_schema: Option<String>,
    pub ledger_table: Option<String>,
    pub module_table: Option<String>,
    #[serde(default)]
    pub install: Vec<FileInstall>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileInstall {
    pub module: String,
    pub version: Option<Version>,
    pub schema: Option<String>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

impl From<FileInstall> for InstallRequest {
    fn from(install: FileInstall) -> Self {
        Self {
            module: install.module,
            version: install.version,
            schema: install.schema,
            variables: install.variables,
        }
    }
}

impl FileConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("invalid config {}", path.display()))
    }
}

/// Fully resolved settings for one run.
#[derive(Debug, Clone)]
pub struct RunConfig {
    pub catalog: PathBuf,
    pub database_url: String,
    pub ledger_schema: String,
    pub ledger_table: String,
    pub module_table: String,
    pub installs: Vec<InstallRequest>,
}

pub struct ConfigInputs<'a> {
    pub config_path: Option<&'a Path>,
    pub catalog: Option<&'a Path>,
    pub database_url: Option<&'a str>,
    pub installs: &'a [String],
    pub ledger_schema: Option<&'a str>,
    pub ledger_table: Option<&'a str>,
    pub module_table: Option<&'a str>,
    /// Set for commands that only read the catalog.
    pub database_optional: bool,
}

impl RunConfig {
    pub fn resolve(inputs: ConfigInputs<'_>) -> Result<Self> {
        let config_path = inputs
            .config_path
            .map(Path::to_path_buf)
            .or_else(|| env_value("FOUNDRY_CONFIG").map(PathBuf::from));

        let file = match &config_path {
            Some(path) => {
                if !path.exists() {
                    bail!("config {} does not exist", path.display());
                }
                FileConfig::load(path)?
            }
            None if Path::new(DEFAULT_CONFIG).exists() => {
                FileConfig::load(Path::new(DEFAULT_CONFIG))?
            }
            None => FileConfig::default(),
        };

        let catalog = inputs
            .catalog
            .map(Path::to_path_buf)
            .or_else(|| env_value("FOUNDRY_CATALOG").map(PathBuf::from))
            .or(file.catalog)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CATALOG));

        let database_url = inputs
            .database_url
            .map(str::to_string)
            .or_else(|| env_value("DATABASE_URL"))
            .or_else(|| env_value("POSTGRES_URL"))
            .or(file.database_url)
            .unwrap_or_default();

        if database_url.is_empty() && !inputs.database_optional {
            bail!("no database configured: set DATABASE_URL or pass --database-url");
        }

        // Explicit specs replace the config file's install list entirely.
        let mut specs: Vec<String> = inputs.installs.to_vec();
        if specs.is_empty() {
            specs = env_value("FOUNDRY_INSTALL")
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
        }

        let installs = if specs.is_empty() {
            file.install.into_iter().map(InstallRequest::from).collect()
        } else {
            specs
                .iter()
                .map(|spec| InstallRequest::parse(spec).map_err(anyhow::Error::from))
                .collect::<Result<Vec<_>>>()?
        };

        if installs.is_empty() {
            bail!(
                "nothing to install: declare [[install]] entries in {DEFAULT_CONFIG}, \
                 set FOUNDRY_INSTALL, or pass --install"
            );
        }

        Ok(Self {
            catalog,
            database_url,
            ledger_schema: pick(
                inputs.ledger_schema,
                "FOUNDRY_LEDGER_SCHEMA",
                file.ledger_schema,
                quench_db::runner::DEFAULT_LEDGER_SCHEMA,
            ),
            ledger_table: pick(
                inputs.ledger_table,
                "FOUNDRY_LEDGER_TABLE",
                file.ledger_table,
                quench_db::runner::DEFAULT_LEDGER_TABLE,
            ),
            module_table: pick(
                inputs.module_table,
                "FOUNDRY_MODULE_TABLE",
                file.module_table,
                quench_db::runner::DEFAULT_MODULE_TABLE,
            ),
            installs,
        })
    }
}

pub fn pick(flag: Option<&str>, env: &str, file: Option<String>, default: &str) -> String {
    flag.map(str::to_string)
        .or_else(|| env_value(env))
        .or(file)
        .unwrap_or_else(|| default.to_string())
}

pub fn env_value(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}
