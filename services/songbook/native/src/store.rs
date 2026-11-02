//! What the app keeps in its data directory: who is signed in (the refresh token sealed with the device's key), and what was
//! played. Files are written whole to a temporary name and renamed into place.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use gatehouse_client::vault;
use quench_serde::{Deserialize, Serialize};
use songbook_core::SongInfo;

/// The data directory's files.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

/// Who is signed in, as kept.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    /// `https://host`, no trailing slash.
    pub server: String,
    pub user: String,
    /// The server asked for no sign-in: no token is sent.
    pub open: bool,
}

/// The song that was playing, and how far into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Last {
    pub id: String,
    pub position_ms: i64,
    pub song: SongInfo,
}

impl Store {
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// `SONGBOOK_DATA`, else `$HOME/.local/share/songbook` (the Android shell sets `HOME` to the app's files directory).
    #[must_use]
    pub fn standard() -> Self {
        if let Some(dir) = std::env::var_os("SONGBOOK_DATA") {
            return Self::new(dir);
        }
        let home = std::env::var_os("HOME").map_or_else(std::env::temp_dir, PathBuf::from);
        Self::new(home.join(".local/share/songbook"))
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn write(&self, name: &str, text: &str, private: bool) {
        let _ = std::fs::create_dir_all(&self.dir);
        let temp = self.dir.join(format!("{name}.tmp"));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        if private {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(not(unix))]
        let _ = private;
        if let Ok(mut file) = options.open(&temp)
            && file.write_all(text.as_bytes()).is_ok()
            && file.sync_all().is_ok()
        {
            let _ = std::fs::rename(&temp, self.dir.join(name));
        }
    }

    fn read(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.join(name)).ok()
    }

    fn remove(&self, name: &str) {
        let _ = std::fs::remove_file(self.dir.join(name));
    }

    #[must_use]
    pub fn settings(&self) -> Settings {
        let mut s = Settings::default();
        for line in self.read("settings").unwrap_or_default().lines() {
            match line.split_once('=') {
                Some(("server", v)) => s.server = v.trim().to_string(),
                Some(("user", v)) => s.user = v.trim().to_string(),
                Some(("open", v)) => s.open = v.trim() == "true",
                _ => {}
            }
        }
        s
    }

    pub fn set_settings(&self, s: &Settings) {
        self.write(
            "settings",
            &format!("server={}\nuser={}\nopen={}\n", s.server, s.user, s.open),
            false,
        );
    }

    /// The refresh token kept by an earlier run (sealed with the device's key when the shell gave one).
    #[must_use]
    pub fn token(&self) -> Option<String> {
        self.read("session")
            .and_then(|t| vault::open(&t))
            .filter(|t| !t.is_empty())
    }

    pub fn set_token(&self, token: &str) {
        self.write("session", &format!("{}\n", vault::seal(token)), true);
    }

    /// Signs out: nothing of the session stays.
    pub fn forget(&self) {
        self.remove("session");
        self.remove("settings");
    }

    // ── What was played ──────────────────────────────────────────────────────

    /// The songs played on this phone, newest first.
    #[must_use]
    pub fn recent(&self) -> Vec<SongInfo> {
        self.read("recent")
            .and_then(|t| quench_serde::json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn add_recent(&self, song: &SongInfo, most: usize) {
        let mut all = vec![song.clone()];
        all.extend(self.recent().into_iter().filter(|s| s.id != song.id));
        all.truncate(most);
        self.write("recent", &quench_serde::json::to_string(&all), false);
    }

    #[must_use]
    pub fn last(&self) -> Option<Last> {
        quench_serde::json::from_str(&self.read("last")?).ok()
    }

    pub fn set_last(&self, last: &Last) {
        self.write("last", &quench_serde::json::to_string(last), false);
    }
}
