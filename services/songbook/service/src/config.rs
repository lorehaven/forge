//! Where things live, from the environment.

use std::path::PathBuf;

/// Paths and tool settings. `home` holds `meta/` (the index, cookies) and `.deezer_cache/`; `data` is the audio folder.
#[derive(Debug, Clone)]
pub struct Config {
    pub home: PathBuf,
    pub data: PathBuf,
    /// Netscape-format YouTube cookies for yt-dlp; used when the file exists.
    pub cookies_file: PathBuf,
    /// Browser whose cookies yt-dlp reads when there is no cookies file (empty: none).
    pub cookies_browser: String,
}

impl Config {
    /// `MUSIC_HOME` (default: the working directory), `MUSIC_DATA` (default `$MUSIC_HOME/data`), `COOKIES_FILE`
    /// (default `$MUSIC_HOME/meta/cookies.txt`), `COOKIES_BROWSER` (default empty).
    #[must_use]
    pub fn from_env() -> Self {
        let home = non_empty("MUSIC_HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
        let data = non_empty("MUSIC_DATA").map_or_else(|| home.join("data"), PathBuf::from);
        let cookies_file = non_empty("COOKIES_FILE")
            .map_or_else(|| home.join("meta").join("cookies.txt"), PathBuf::from);
        Self {
            home,
            data,
            cookies_file,
            cookies_browser: quench_core::env::get_or("COOKIES_BROWSER", ""),
        }
    }

    #[must_use]
    pub fn meta(&self) -> PathBuf {
        self.home.join("meta")
    }

    #[must_use]
    pub fn db(&self) -> PathBuf {
        self.meta().join("library.db")
    }

    /// The listeners' playlists (apart from the library index: writing one must not drop that index's cached copy).
    #[must_use]
    pub fn user_db(&self) -> PathBuf {
        self.meta().join("user.db")
    }

    #[must_use]
    pub fn cache(&self) -> PathBuf {
        self.home.join(".deezer_cache")
    }
}

fn non_empty(name: &str) -> Option<String> {
    let value = quench_core::env::get_or(name, "");
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}
