//! `~/.config/gauge/config.toml`: the server and the signed-in session's token, so the command needs no arguments.
//!
//! ```toml
//! url = "https://ennor.ddns.net"
//! token = "..."          # written by `gauge login`, and renewed by every run
//! path = "/gauge"        # optional
//! ```
//!
//! The token is a Gatehouse refresh token. Gatehouse rotates it whenever it is used, so each new one is written back
//! here before the request that needed it; the file is kept readable by its owner only.

use quench_core::{Context, Result};
use quench_serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// Where the file is: under `$XDG_CONFIG_HOME`, else `$HOME/.config`.
#[must_use]
pub fn config_path(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let base = env("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("gauge").join("config.toml"))
}

impl FileConfig {
    /// A missing file is the common case and is not an error; one that exists but does not parse is, since
    /// ignoring it would leave a typo'd file silently unused.
    ///
    /// # Errors
    /// The file exists and cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).with_context(|| format!("failed to read {}", path.display())),
        };
        quench_serde::toml::from_str(&text)
            .with_context(|| format!("failed to parse {} as TOML", path.display()))
    }

    /// Writes the file whole through a temporary one, so a crash never leaves half a token, readable by the owner
    /// only.
    ///
    /// # Errors
    /// The directory or file cannot be written.
    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path.parent().context("the config path has no directory")?;
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
        let text =
            quench_serde::toml::to_string_pretty(self).context("failed to write the config")?;
        let temp = path.with_extension("toml.tmp");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp)
            .with_context(|| format!("failed to write {}", temp.display()))?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .with_context(|| format!("failed to write {}", temp.display()))?;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to protect {}", temp.display()))?;
        std::fs::rename(&temp, path)
            .with_context(|| format!("failed to replace {}", path.display()))
    }

    /// Replaces the token in the file at `path`, keeping what else it holds.
    ///
    /// # Errors
    /// As [`load`](Self::load) and [`save`](Self::save).
    pub fn store_token(path: &Path, token: &str) -> Result<()> {
        let mut file = Self::load(path)?;
        file.token = Some(token.to_string());
        file.save(path)
    }
}
