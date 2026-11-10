use quench_cli::prelude::{Tone, print_status};
use quench_core::Error;
use quench_serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse TOML: {0}")]
    ParseError(#[from] quench_serde::Error),
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
    /// Seconds between runs under `pulley daemon`; jobs without this are
    /// REPL-only and invisible to the daemon.
    #[serde(default)]
    pub interval: Option<u64>,
    /// Optional: a path relative to `src` that is always present while the source's data is (an existing file or directory;
    /// nothing has to be created). A job with a marker syncs only when the source answers and the marker is in it. Without
    /// one, a job that deletes still needs a source that exists and is not empty; see [`crate::source`].
    #[serde(default)]
    pub marker: Option<String>,
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
        let config: Self = quench_serde::toml::from_str(&contents)?;
        Ok(config)
    }

    #[must_use]
    pub fn global_config_dir() -> Option<PathBuf> {
        home_dir().map(|home| home.join(".config/pulley"))
    }

    #[must_use]
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
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml")
            })
            .collect();

        configs.sort();
        configs
    }

    #[must_use]
    pub fn find_local_configs() -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(".") else {
            return Vec::new();
        };

        let mut configs: Vec<PathBuf> = entries
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|name| name.ends_with(".pulley.toml"))
            })
            .collect();

        configs.sort();
        configs
    }

    pub fn merge_job(jobs: &mut Vec<Job>, new_job: Job) {
        if let Some(existing_job) = jobs.iter_mut().find(|j| j.id == new_job.id) {
            // Overwrite existing job with same ID
            *existing_job = new_job;
        } else {
            // Append new job
            jobs.push(new_job);
        }
    }

    pub fn load_merged() -> Result<Self, ConfigError> {
        Self::load(true)
    }

    /// [`Config::load_merged`] without saying which files were read, and with its complaints on standard error: for a command
    /// whose standard output is the answer (`pulley status --json`).
    pub fn load_merged_quietly() -> Result<Self, ConfigError> {
        Self::load(false)
    }

    fn load(verbose: bool) -> Result<Self, ConfigError> {
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
                    let message = format!("failed to load {}: {}", config_path.display(), e);
                    if verbose {
                        print_status(Tone::Warn, "pulley", &message);
                    } else {
                        eprintln!("pulley {message}");
                    }
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
                    let message = format!("failed to load {}: {}", config_path.display(), e);
                    if verbose {
                        print_status(Tone::Warn, "pulley", &message);
                    } else {
                        eprintln!("pulley {message}");
                    }
                }
            }
        }

        if merged_jobs.is_empty() {
            let search_info = format!(
                "Global: {}/*.toml, Local: *.pulley.toml",
                Self::global_config_dir().map_or_else(
                    || "~/.config/pulley".to_string(),
                    |p| p.display().to_string()
                )
            );
            return Err(ConfigError::NotFound(search_info));
        }

        if verbose && !loaded_files.is_empty() {
            print_status(Tone::Info, "pulley", "loaded configuration from:");
            for file in &loaded_files {
                println!("  - {file}");
            }
        }

        Ok(Self { jobs: merged_jobs })
    }
}

/// `HOME` is unset by default in many Windows shells; `USERPROFILE` is its
/// equivalent there.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
