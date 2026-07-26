use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(String),
    #[error("Failed to parse config: {0}")]
    ParseError(String),
    #[error("Missing required config value: {0}")]
    MissingValue(String),
    #[error("Invalid config value: {0}")]
    InvalidValue(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

/// Configuration loader supporting multiple formats and sources
pub struct ConfigLoader {
    env_prefix: String,
}

impl ConfigLoader {
    pub fn new(env_prefix: &str) -> Self {
        Self {
            env_prefix: env_prefix.to_string(),
        }
    }

    /// Load from JSON file
    pub fn from_json_file<T: DeserializeOwned>(path: &str) -> Result<T> {
        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError(format!("{}: {}", path, e)))?;

        serde_json::from_str(&content)
            .map_err(|e| ConfigError::ParseError(format!("JSON parse error: {}", e)))
    }

    /// Load from YAML file
    pub fn from_yaml_file<T: DeserializeOwned>(path: &str) -> Result<T> {
        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError(format!("{}: {}", path, e)))?;

        serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(format!("YAML parse error: {}", e)))
    }

    /// Load from TOML file
    pub fn from_toml_file<T: DeserializeOwned>(path: &str) -> Result<T> {
        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError(format!("{}: {}", path, e)))?;

        toml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(format!("TOML parse error: {}", e)))
    }

    /// Try loading from multiple sources in order (file -> env var)
    pub fn load_with_fallback<T: DeserializeOwned + Serialize>(
        file_path: Option<&str>,
        env_var: &str,
        default: Option<T>,
    ) -> Result<T> {
        // Try file first
        if let Some(path) = file_path
            && Path::new(path).exists()
        {
            if path.ends_with(".json") {
                return Self::from_json_file(path);
            } else if path.ends_with(".yaml") || path.ends_with(".yml") {
                return Self::from_yaml_file(path);
            } else if path.ends_with(".toml") {
                return Self::from_toml_file(path);
            }
        }

        // Try environment variable
        if let Ok(content) = std::env::var(env_var)
            && let Ok(config) = serde_json::from_str::<T>(&content)
        {
            return Ok(config);
        }

        // Use default or error
        default.ok_or_else(|| {
            ConfigError::MissingValue(format!("No config found in file or env var {}", env_var))
        })
    }

    /// Get environment variable with optional prefix
    pub fn env_string(&self, key: &str, default: &str) -> String {
        let prefixed_key = format!("{}_{}", self.env_prefix, key);
        std::env::var(&prefixed_key)
            .or_else(|_| std::env::var(key))
            .unwrap_or_else(|_| default.to_string())
    }

    /// Get environment variable as u64
    pub fn env_u64(&self, key: &str, default: u64) -> u64 {
        let value = self.env_string(key, &default.to_string());
        value.parse().unwrap_or(default)
    }

    /// Get environment variable as bool
    pub fn env_bool(&self, key: &str, default: bool) -> bool {
        let value = self.env_string(key, &default.to_string()).to_lowercase();
        matches!(value.as_str(), "true" | "1" | "yes" | "on")
    }

    /// Get environment variable split by comma
    pub fn env_list(&self, key: &str, default: &[&str]) -> Vec<String> {
        let value = self.env_string(key, &default.join(","));
        value
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// Generic configuration with environment variable overrides
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub service_name: String,
    pub port: u16,
    pub database_url: String,
    pub log_level: String,
    #[serde(default)]
    pub debug: bool,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let loader = ConfigLoader::new("APP");
        Self {
            service_name: loader.env_string("SERVICE_NAME", "unknown"),
            port: loader.env_u64("PORT", 8080) as u16,
            database_url: loader.env_string("DATABASE_URL", ""),
            log_level: loader.env_string("LOG_LEVEL", "info"),
            debug: loader.env_bool("DEBUG", false),
        }
    }
}
