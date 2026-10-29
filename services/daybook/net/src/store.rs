//! What the app keeps in its data directory: the session and the few settings. Files are written whole to a temporary
//! name and renamed into place. The session file can be read only by the user.

use std::io::Write;
use std::path::{Path, PathBuf};

use daybook_core::ListRef;

/// What the person chose and the app should remember.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    /// Where the services are (`https://host`, no trailing slash).
    pub server: String,
    pub user: String,
    /// The view that was open.
    pub list: ListRef,
}

impl Settings {
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            match line.split_once('=') {
                Some(("server", v)) => s.server = v.trim().to_string(),
                Some(("user", v)) => s.user = v.trim().to_string(),
                Some(("list", v)) => s.list = ListRef::parse(v.trim()),
                _ => {}
            }
        }
        s
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        format!(
            "server={}\nuser={}\nlist={}\n",
            self.server,
            self.user,
            self.list.key(),
        )
    }
}

/// The data directory's files.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// `$XDG_DATA_HOME/daybook`, else `$HOME/.local/share/daybook`. (On Android the host sets `HOME` to the app's files
    /// directory.)
    #[must_use]
    pub fn standard() -> Self {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(std::env::temp_dir);
        Self::new(base.join("daybook"))
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn write(&self, name: &str, text: &str, private: bool) {
        let _ = std::fs::create_dir_all(&self.dir);
        let target = self.dir.join(name);
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
            let _ = std::fs::rename(&temp, &target);
        }
    }

    fn read(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.join(name)).ok()
    }

    /// The refresh token kept by an earlier run (or by the other program of this app: the widget's refresh).
    #[must_use]
    pub fn token(&self) -> Option<String> {
        self.read("session")
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    }

    pub fn set_token(&self, token: &str) {
        self.write("session", &format!("{token}\n"), true);
    }

    /// Signs out: the token goes.
    pub fn clear_token(&self) {
        let _ = std::fs::remove_file(self.dir.join("session"));
    }

    /// What the home-screen widget draws (see `daybook_core::widget`), kept where the widget's Kotlin reads it.
    pub fn set_widget_snapshot(&self, json: &str) {
        self.write("widget.json", json, false);
    }

    /// No widget content: nobody is signed in.
    pub fn clear_widget_snapshot(&self) {
        let _ = std::fs::remove_file(self.dir.join("widget.json"));
    }

    /// What the host (a tap on the widget's **+**) asked the program to show, once: the file is removed as it is read.
    #[must_use]
    pub fn take_launch(&self) -> Option<String> {
        let text = self.read("launch")?;
        let _ = std::fs::remove_file(self.dir.join("launch"));
        Some(text)
    }

    #[must_use]
    pub fn settings(&self) -> Settings {
        self.read("settings")
            .map(|t| Settings::from_text(&t))
            .unwrap_or_default()
    }

    pub fn set_settings(&self, settings: &Settings) {
        self.write("settings", &settings.to_text(), false);
    }
}
