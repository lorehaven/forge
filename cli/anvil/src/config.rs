use anyhow::Result;
use quench_cli::prelude::{Tone, print_status};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub docker: DockerConfig,

    #[serde(default)]
    pub install: InstallConfig,

    #[serde(default)]
    pub publish: PublishConfig,
}

#[derive(Debug, Default, Deserialize)]
pub struct DockerConfig {
    #[serde(default)]
    pub registry: String,
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
    pub image_name: Option<String>,

    #[serde(default)]
    pub registries: Vec<String>,

    // Deprecated single-registry override, kept for backward compatibility.
    #[serde(default)]
    pub registry: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct InstallConfig {
    /// List of packages to install
    #[serde(default)]
    pub packages: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PublishConfig {
    /// Cargo registry to publish to
    #[serde(default)]
    pub registry: String,
    /// List of publishable packages
    #[serde(default)]
    pub packages: Vec<String>,
}

pub fn load_config() -> Result<Config> {
    let config = fs::read_to_string(".anvil.toml").map_or_else(
        |_| {
            print_status(
                Tone::Warn,
                "anvil",
                "failed to read .anvil.toml, defaulting to empty config",
            );
            Config::default()
        },
        |content| {
            toml::from_str(&content).unwrap_or_else(|err| {
                print_status(
                    Tone::Warn,
                    "anvil",
                    &format!("failed to parse .anvil.toml ({err}), defaulting to empty config"),
                );
                Config::default()
            })
        },
    );

    Ok(config)
}
