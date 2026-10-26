use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct RiveterConfig {
    /// Warehouse including its base path, e.g. `https://host/warehouse`: where rivets are published
    /// and pulled. `RIVETER_WAREHOUSE_URL` wins over it.
    pub warehouse_url: Option<String>,
    #[serde(default)]
    pub env: EnvConfig,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct EnvConfig {
    pub current: Option<String>,
}

use quench_core::{Context, Result};
use std::fs;

const CONFIG_FILE: &str = ".riveter.toml";

pub fn load_config() -> Result<RiveterConfig> {
    let Ok(content) = fs::read_to_string(CONFIG_FILE) else {
        return Ok(RiveterConfig::default());
    };

    quench_serde::toml::from_str(&content).context("Failed to parse .riveter.toml")
}

pub fn save_config(config: &RiveterConfig) -> Result<()> {
    let content = quench_serde::toml::to_string_pretty(config)?;
    fs::write(CONFIG_FILE, content).context("Failed to write .riveter.toml")
}
