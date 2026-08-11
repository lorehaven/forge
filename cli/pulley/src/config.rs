use quench_cli::prelude::{Tone, print_status};
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
    /// Seconds between runs under `pulley daemon`; jobs without this are
    /// REPL-only and invisible to the daemon.
    #[serde(default)]
    pub interval: Option<u64>,
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
        home_dir().map(|home| home.join(".config/pulley"))
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
                    print_status(
                        Tone::Warn,
                        "pulley",
                        &format!("failed to load {}: {}", config_path.display(), e),
                    );
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
                    print_status(
                        Tone::Warn,
                        "pulley",
                        &format!("failed to load {}: {}", config_path.display(), e),
                    );
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
            print_status(Tone::Info, "pulley", "loaded configuration from:");
            for file in &loaded_files {
                println!("  - {}", file);
            }
        }

        Ok(Config { jobs: merged_jobs })
    }
}

/// `HOME` is unset by default in many Windows shells; `USERPROFILE` is its
/// equivalent there.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // `home_dir` and `find_local_configs` read process-wide state (env vars,
    // the current directory) that every #[test] in this binary shares. This
    // lock keeps the tests below from interleaving those mutations with each
    // other; it's only safe because HOME/USERPROFILE and the cwd aren't
    // touched anywhere else in this crate's tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            // SAFETY: serialized by ENV_LOCK, held for the guard's lifetime.
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = std::env::var_os(key);
            // SAFETY: serialized by ENV_LOCK, held for the guard's lifetime.
            unsafe { std::env::remove_var(key) };
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: serialized by ENV_LOCK, held for the guard's lifetime.
            unsafe {
                match &self.previous {
                    Some(value) => std::env::set_var(self.key, value),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pulley-config-test-{name}-{}", std::process::id()))
    }

    #[test]
    fn from_file_errors_when_the_file_is_missing() {
        let path = scratch("missing.toml");
        let err = Config::from_file(&path).unwrap_err();
        assert!(matches!(err, ConfigError::NotFound(_)));
    }

    #[test]
    fn from_file_errors_on_malformed_toml() {
        let path = scratch("malformed.toml");
        std::fs::write(&path, "not = [valid").unwrap();
        let err = Config::from_file(&path).unwrap_err();
        assert!(matches!(err, ConfigError::ParseError(_)));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn from_file_parses_a_valid_job() {
        let path = scratch("valid.toml");
        std::fs::write(
            &path,
            r#"
            [[jobs]]
            id = "photos"
            desc = "backup photos"
            src = "/home/me/photos"
            dest = "/backup/photos"
            interval = 3600
            "#,
        )
        .unwrap();

        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.jobs.len(), 1);
        assert_eq!(config.jobs[0].id, "photos");
        assert_eq!(config.jobs[0].interval, Some(3600));
        assert!(!config.jobs[0].delete);
        assert!(!config.jobs[0].no_confirm);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn merge_job_appends_a_new_id() {
        let mut jobs = vec![job("a"), job("b")];
        Config::merge_job(&mut jobs, job("c"));
        assert_eq!(jobs.len(), 3);
        assert_eq!(jobs[2].id, "c");
    }

    #[test]
    fn merge_job_overwrites_an_existing_id_in_place() {
        let mut jobs = vec![job("a"), job("b")];
        let mut replacement = job("b");
        replacement.desc = "replaced".to_string();

        Config::merge_job(&mut jobs, replacement);

        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[1].desc, "replaced");
    }

    fn job(id: &str) -> Job {
        Job {
            id: id.to_string(),
            desc: "desc".to_string(),
            src: "/src".to_string(),
            dest: "/dest".to_string(),
            delete: false,
            skip: Vec::new(),
            no_confirm: false,
            interval: None,
        }
    }

    #[test]
    fn home_dir_prefers_home_over_userprofile() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _home = EnvGuard::set("HOME", "/home/me");
        let _profile = EnvGuard::set("USERPROFILE", "C:\\Users\\me");

        assert_eq!(home_dir(), Some(PathBuf::from("/home/me")));
    }

    #[test]
    fn home_dir_falls_back_to_userprofile_without_home() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _home = EnvGuard::unset("HOME");
        let _profile = EnvGuard::set("USERPROFILE", "C:\\Users\\me");

        assert_eq!(home_dir(), Some(PathBuf::from("C:\\Users\\me")));
    }

    #[test]
    fn home_dir_is_none_without_either_variable() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _home = EnvGuard::unset("HOME");
        let _profile = EnvGuard::unset("USERPROFILE");

        assert_eq!(home_dir(), None);
    }

    #[test]
    fn find_local_configs_matches_only_dot_pulley_toml_suffix() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = scratch("local-configs-dir");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("backup.pulley.toml"), "jobs = []").unwrap();
        std::fs::write(dir.join("unrelated.toml"), "jobs = []").unwrap();
        std::fs::write(dir.join("also.pulley.toml"), "jobs = []").unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let found = Config::find_local_configs();
        std::env::set_current_dir(original_cwd).unwrap();

        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["also.pulley.toml", "backup.pulley.toml"]);

        std::fs::remove_dir_all(&dir).ok();
    }
}
