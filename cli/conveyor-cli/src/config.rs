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

pub fn config_path() -> Option<PathBuf> {
    if let Some(xdg) = non_empty_env("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("conveyor").join("config.toml"));
    }
    let home = non_empty_env("HOME")?;
    Some(PathBuf::from(home).join(".config/conveyor/config.toml"))
}

pub fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}
