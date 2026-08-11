//! `~/.config/conveyor/config.toml` - the lowest-priority source for the
//! same handful of settings `Client::new` already reads from CLI flags and
//! env vars. Precedence, low to high: config file, env var, CLI flag - a
//! flag on the command line always wins, matching how `CONVEYOR_INSECURE`
//! already only ever adds to a flag rather than overriding it.
//!
//! No `dirs`/`directories` crate in this workspace, so `$XDG_CONFIG_HOME` is
//! resolved by hand the same way every other path in this crate comes from
//! `envmnt`/`std::env`.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize)]
pub struct FileConfig {
    pub url: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub gatehouse_url: Option<String>,
    #[serde(default)]
    pub insecure: bool,
}

impl FileConfig {
    /// Missing config file is not an error - it is the common case, since the
    /// file is optional. A config file that exists but does not parse *is*
    /// an error: staying silent there would leave someone editing a typo'd
    /// TOML wondering why their settings are simply never used.
    pub fn load() -> Result<Self> {
        let Some(path) = config_path() else {
            return Ok(Self::default());
        };

        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => {
                return Err(err).with_context(|| format!("failed to read {}", path.display()));
            }
        };

        toml::from_str(&contents)
            .with_context(|| format!("failed to parse {} as TOML", path.display()))
    }
}

fn config_path() -> Option<PathBuf> {
    if let Some(xdg) = non_empty_env("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("conveyor").join("config.toml"));
    }
    let home = non_empty_env("HOME")?;
    Some(PathBuf::from(home).join(".config/conveyor/config.toml"))
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // `config_path` and `non_empty_env` read real process environment
    // variables, which every #[test] in this binary shares. This lock keeps
    // the tests below from interleaving their `set_var`/`remove_var` calls
    // with each other; it does not (and cannot) protect against another
    // module's tests touching the same names, so it's only safe because
    // XDG_CONFIG_HOME/HOME aren't touched anywhere else in this crate.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: serialized by ENV_LOCK, held for the guard's lifetime.
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
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

    #[test]
    fn non_empty_env_is_none_for_a_missing_var() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::unset("CONVEYOR_CONFIG_TEST_MISSING");
        assert_eq!(non_empty_env("CONVEYOR_CONFIG_TEST_MISSING"), None);
    }

    #[test]
    fn non_empty_env_is_none_for_an_empty_var() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::set("CONVEYOR_CONFIG_TEST_EMPTY", "");
        assert_eq!(non_empty_env("CONVEYOR_CONFIG_TEST_EMPTY"), None);
    }

    #[test]
    fn non_empty_env_returns_a_set_value() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::set("CONVEYOR_CONFIG_TEST_SET", "hello");
        assert_eq!(
            non_empty_env("CONVEYOR_CONFIG_TEST_SET"),
            Some("hello".to_string())
        );
    }

    #[test]
    fn config_path_prefers_xdg_config_home_over_home() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _xdg = EnvGuard::set("XDG_CONFIG_HOME", "/xdg-root");
        let _home = EnvGuard::set("HOME", "/home-root");

        assert_eq!(
            config_path(),
            Some(PathBuf::from("/xdg-root/conveyor/config.toml"))
        );
    }

    #[test]
    fn config_path_falls_back_to_home_without_xdg() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _xdg = EnvGuard::unset("XDG_CONFIG_HOME");
        let _home = EnvGuard::set("HOME", "/home-root");

        assert_eq!(
            config_path(),
            Some(PathBuf::from("/home-root/.config/conveyor/config.toml"))
        );
    }

    #[test]
    fn config_path_is_none_without_either_variable() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _xdg = EnvGuard::unset("XDG_CONFIG_HOME");
        let _home = EnvGuard::unset("HOME");

        assert_eq!(config_path(), None);
    }

    #[test]
    fn config_path_treats_an_empty_xdg_as_unset() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _xdg = EnvGuard::set("XDG_CONFIG_HOME", "");
        let _home = EnvGuard::set("HOME", "/home-root");

        assert_eq!(
            config_path(),
            Some(PathBuf::from("/home-root/.config/conveyor/config.toml"))
        );
    }

    #[test]
    fn load_returns_defaults_when_no_config_file_exists() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "conveyor-config-test-missing-{}",
            std::process::id()
        ));
        let _xdg = EnvGuard::set("XDG_CONFIG_HOME", &dir.display().to_string());

        let config = FileConfig::load().unwrap();
        assert_eq!(config.url, None);
        assert!(!config.insecure);
    }

    #[test]
    fn load_parses_a_valid_config_file() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir =
            std::env::temp_dir().join(format!("conveyor-config-test-valid-{}", std::process::id()));
        let conveyor_dir = dir.join("conveyor");
        std::fs::create_dir_all(&conveyor_dir).unwrap();
        std::fs::write(
            conveyor_dir.join("config.toml"),
            "url = \"https://localhost:9443/conveyor\"\ninsecure = true\n",
        )
        .unwrap();
        let _xdg = EnvGuard::set("XDG_CONFIG_HOME", &dir.display().to_string());

        let config = FileConfig::load().unwrap();
        assert_eq!(
            config.url.as_deref(),
            Some("https://localhost:9443/conveyor")
        );
        assert!(config.insecure);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_errors_on_malformed_toml() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "conveyor-config-test-malformed-{}",
            std::process::id()
        ));
        let conveyor_dir = dir.join("conveyor");
        std::fs::create_dir_all(&conveyor_dir).unwrap();
        std::fs::write(conveyor_dir.join("config.toml"), "not = [valid").unwrap();
        let _xdg = EnvGuard::set("XDG_CONFIG_HOME", &dir.display().to_string());

        let err = FileConfig::load().unwrap_err();
        assert!(err.to_string().contains("failed to parse"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
