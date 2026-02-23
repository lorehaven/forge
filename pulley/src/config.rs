use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse TOML: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("Config file not found: {0}")]
    NotFound(String),
}

#[derive(Clone, Debug, Deserialize)]
pub struct Job {
    pub id: String,
    pub desc: String,
    pub src: String,
    pub dest: String,
    #[serde(default)]
    pub delete: bool,
    #[serde(default)]
    pub skip: Vec<String>,
    #[serde(default)]
    #[serde(rename = "no-confirm")]
    pub no_confirm: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub jobs: Vec<Job>,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(ConfigError::NotFound(path.display().to_string()));
        }
        let contents = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn global_config_dir() -> Option<PathBuf> {
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(format!("{}/.config/pulley", home)))
    }

    pub fn find_global_configs() -> Vec<PathBuf> {
        let Some(config_dir) = Self::global_config_dir() else {
            return Vec::new();
        };

        if !config_dir.exists() {
            return Vec::new();
        }

        let Ok(entries) = fs::read_dir(&config_dir) else {
            return Vec::new();
        };

        let mut configs: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml")
            })
            .collect();

        configs.sort();
        configs
    }

    pub fn find_local_configs() -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(".") else {
            return Vec::new();
        };

        let mut configs: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .map(|name| name.ends_with(".pulley.toml"))
                        .unwrap_or(false)
            })
            .collect();

        configs.sort();
        configs
    }

    fn merge_job(jobs: &mut Vec<Job>, new_job: Job) {
        if let Some(existing_job) = jobs.iter_mut().find(|j| j.id == new_job.id) {
            // Overwrite existing job with same ID
            *existing_job = new_job;
        } else {
            // Append new job
            jobs.push(new_job);
        }
    }

    pub fn load_merged() -> Result<Self, ConfigError> {
        let mut merged_jobs = Vec::new();
        let mut loaded_files = Vec::new();

        // Load all global configs
        let global_configs = Self::find_global_configs();
        for config_path in global_configs {
            match Self::from_file(&config_path) {
                Ok(config) => {
                    for job in config.jobs {
                        Self::merge_job(&mut merged_jobs, job);
                    }
                    loaded_files.push(config_path.display().to_string());
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load {}: {}", config_path.display(), e);
                }
            }
        }

        // Load all local configs (these override globals)
        let local_configs = Self::find_local_configs();
        for config_path in local_configs {
            match Self::from_file(&config_path) {
                Ok(config) => {
                    for job in config.jobs {
                        Self::merge_job(&mut merged_jobs, job);
                    }
                    loaded_files.push(config_path.display().to_string());
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load {}: {}", config_path.display(), e);
                }
            }
        }

        if merged_jobs.is_empty() {
            let search_info = format!(
                "Global: {}/*.toml, Local: *.pulley.toml",
                Self::global_config_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "~/.config/pulley".to_string())
            );
            return Err(ConfigError::NotFound(search_info));
        }

        if !loaded_files.is_empty() {
            println!("Loaded configuration from:");
            for file in &loaded_files {
                println!("  - {}", file);
            }
        }

        Ok(Config { jobs: merged_jobs })
    }
}
