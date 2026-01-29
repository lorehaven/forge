use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub modules: HashMap<String, ModuleConfig>,
    pub skipped: Option<SkippedConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ModuleConfig {
    pub packages: Vec<String>,
    pub dockerfile: String,

    #[serde(default)]
    pub package_dockerfiles: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct SkippedConfig {
    pub modules: Vec<String>,
}

pub fn load_config() -> Result<Config> {
    let config = fs::read_to_string(".anvil.toml").map_or_else(
        |_| {
            eprintln!("⚠️  Failed to read .anvil.toml, defaulting to empty config");
            Config::default()
        },
        |content| {
            toml::from_str(&content).unwrap_or_else(|err| {
                eprintln!("⚠️  Failed to parse .anvil.toml ({err}), defaulting to empty config");
                Config::default()
            })
        },
    );

    Ok(config)
}
