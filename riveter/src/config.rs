use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct RiveterConfig {
    #[serde(default)]
    pub env: EnvConfig,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct EnvConfig {
    pub current: Option<String>,
}

use anyhow::{Context, Result};
use std::fs;

const CONFIG_FILE: &str = ".riveter.toml";

pub fn load_config() -> Result<RiveterConfig> {
    let content = match fs::read_to_string(CONFIG_FILE) {
        Ok(c) => c,
        Err(_) => return Ok(RiveterConfig::default()),
    };

    toml::from_str(&content).context("Failed to parse .riveter.toml")
}

pub fn save_config(config: &RiveterConfig) -> Result<()> {
    let content = toml::to_string_pretty(config)?;
    fs::write(CONFIG_FILE, content).context("Failed to write .riveter.toml")
}
