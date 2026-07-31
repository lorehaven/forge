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
    pub debug: bool,
    /// Base URL of a switchboard-service instance (e.g.
    /// `https://localhost:7443/switchboard`). Required when `kind = "switchboard"`.
    pub switchboard_url: Option<String>,
    /// Set to `false` to accept switchboard's self-signed dev certificate.
    #[serde(default = "default_switchboard_tls_verify")]
    pub switchboard_tls_verify: bool,
}

const fn default_switchboard_tls_verify() -> bool {
    true
}

/// Whether verbose/debug logging is enabled, via `WELDER_DEBUG` or
/// `[backend].debug`. Shared so every module gates its debug output the
/// same way instead of each re-deriving its own notion of "verbose".
#[must_use]
pub fn is_verbose() -> bool {
    if let Ok(val) = std::env::var("WELDER_DEBUG")
        && (val.eq_ignore_ascii_case("true") || val == "1")
    {
        return true;
    }
    CONFIG.backend.debug
}

impl Config {
    fn config_file_path() -> Result<PathBuf, anyhow::Error> {
        let cwd = std::env::current_dir().context("Cannot determine current working directory")?;
        Ok(cwd.join(".welder").join("config.toml"))
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
                eprintln!("Warning: Invalid .welder.toml: {e}. Using defaults.");
                Self::default()
            }),
            Err(e) => {
                eprintln!("Warning: Failed to read .welder.toml: {e}. Using defaults.");
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
            debug: false,
            switchboard_url: None,
            switchboard_tls_verify: true,
        }
    }
}
