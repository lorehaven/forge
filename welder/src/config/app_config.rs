use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;

pub static CONFIG: LazyLock<Config> = LazyLock::new(Config::load);

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub backend: BackendConfig,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct BackendConfig {
    pub kind: String,
    pub ollama_url: Option<String>,
}

impl Config {
    fn config_file_path() -> Result<PathBuf, anyhow::Error> {
        let cwd = std::env::current_dir().context("Cannot determine current working directory")?;
        Ok(cwd.join(".forge").join("config.toml"))
    }

    #[must_use]
    pub fn load() -> Self {
        let Ok(config_path) = Self::config_file_path() else {
            return Self::default();
        };

        if !config_path.exists() {
            return Self::default();
        }

        match fs::read_to_string(&config_path) {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!("Warning: Invalid .foundry.toml: {e}. Using defaults.");
                Self::default()
            }),
            Err(e) => {
                eprintln!("Warning: Failed to read .foundry.toml: {e}. Using defaults.");
                Self::default()
            }
        }
    }
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            kind: "ollama".to_string(),
            ollama_url: Some("127.0.0.1:11434".to_string()),
        }
    }
}
