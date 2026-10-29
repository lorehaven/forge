//! What the person chose: which server, which storage, what to back up and under which conditions. Kept as plain
//! `key=value` lines (a folder is one line of its own); unknown lines are ignored, so a newer file is readable by an
//! older app.

use crate::manifest::{escape, unescape};

/// The categories that are not folders.
pub const PHOTOS: &str = "photos";
pub const VIDEOS: &str = "videos";

/// A folder picked once and backed up under its own category, `custom:<slug>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub slug: String,
    pub name: String,
    /// The tree the system's picker granted.
    pub uri: String,
    pub enabled: bool,
    pub wifi_only: bool,
}

impl Folder {
    #[must_use]
    pub fn category(&self) -> String {
        format!("custom:{}", self.slug)
    }
}

/// The person's choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `https://host`; gatehouse is at `/gatehouse`, warehouse at `/warehouse`.
    pub server: String,
    /// The warehouse storage this device writes to.
    pub storage: String,
    /// Whether the person chose that storage (from the ones the account can use). Nothing is sent before they do: the
    /// default is a guess from the device's model.
    pub storage_confirmed: bool,
    /// Whether a backup runs by itself (when new files appear and every so often) and not only when asked.
    pub auto: bool,
    pub photos: bool,
    pub videos: bool,
    pub videos_wifi_only: bool,
    /// Whether files over [`crate::LARGE_FILE_BYTES`] wait for the charger.
    pub charging_for_large: bool,
    /// How many files are sent at once (1 to 5).
    pub concurrency: usize,
    pub folders: Vec<Folder>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server: String::new(),
            storage: String::new(),
            storage_confirmed: false,
            auto: true,
            photos: true,
            videos: true,
            videos_wifi_only: false,
            charging_for_large: true,
            concurrency: 3,
            folders: Vec::new(),
        }
    }
}

/// A storage the account may use, as the chooser shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageInfo {
    pub name: String,
    /// Bytes used and allowed, where the server says.
    pub used: Option<u64>,
    pub quota: Option<u64>,
}

/// A storage name from a device's model: lowercase letters, digits and `_` (`Moto Edge 60 Pro` becomes
/// `moto_edge_60_pro`).
#[must_use]
pub fn storage_name(model: &str) -> String {
    let mut name = String::new();
    for c in model.trim().chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c.to_ascii_lowercase());
        } else if !name.ends_with('_') && !name.is_empty() {
            name.push('_');
        }
    }
    let name = name.trim_end_matches('_').to_string();
    if name.is_empty() {
        "device".to_string()
    } else {
        name
    }
}

/// A slug for a folder's name, unique among `taken`.
#[must_use]
pub fn folder_slug(name: &str, taken: &[Folder]) -> String {
    let base = storage_name(name);
    let mut slug = base.clone();
    let mut n = 2;
    while taken.iter().any(|f| f.slug == slug) {
        slug = format!("{base}_{n}");
        n += 1;
    }
    slug
}

/// `https://host` from what was typed: trimmed, no trailing slash.
#[must_use]
pub fn clean_server(typed: &str) -> String {
    typed.trim().trim_end_matches('/').to_string()
}

impl Settings {
    #[must_use]
    pub fn to_text(&self) -> String {
        let flag = |b: bool| if b { "1" } else { "0" };
        let mut out = format!(
            "server={}\nstorage={}\nstorage_confirmed={}\nauto={}\nphotos={}\nvideos={}\nvideos_wifi_only={}\ncharging_for_large={}\nconcurrency={}\n",
            escape(&self.server),
            escape(&self.storage),
            flag(self.storage_confirmed),
            flag(self.auto),
            flag(self.photos),
            flag(self.videos),
            flag(self.videos_wifi_only),
            flag(self.charging_for_large),
            self.concurrency,
        );
        for f in &self.folders {
            out.push_str(&format!(
                "folder={}\t{}\t{}\t{}\t{}\n",
                escape(&f.slug),
                escape(&f.name),
                escape(&f.uri),
                flag(f.enabled),
                flag(f.wifi_only)
            ));
        }
        out
    }

    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let on = value == "1";
            match key {
                "server" => s.server = unescape(value),
                "storage" => s.storage = unescape(value),
                "storage_confirmed" => s.storage_confirmed = on,
                "auto" => s.auto = on,
                "photos" => s.photos = on,
                "videos" => s.videos = on,
                "videos_wifi_only" => s.videos_wifi_only = on,
                "charging_for_large" => s.charging_for_large = on,
                "concurrency" => s.concurrency = value.parse().map_or(3, |n: usize| n.clamp(1, 5)),
                "folder" => {
                    let f: Vec<&str> = value.split('\t').collect();
                    if f.len() == 5 {
                        s.folders.push(Folder {
                            slug: unescape(f[0]),
                            name: unescape(f[1]),
                            uri: unescape(f[2]),
                            enabled: f[3] == "1",
                            wifi_only: f[4] == "1",
                        });
                    }
                }
                _ => {}
            }
        }
        s
    }

    /// The categories a scan looks at, with the kind of source each one is.
    #[must_use]
    pub fn categories(&self) -> Vec<Category> {
        let mut list = Vec::new();
        if self.photos {
            list.push(Category {
                key: PHOTOS.into(),
                label: "Photos".into(),
                wifi_only: false,
                folder: None,
            });
        }
        if self.videos {
            list.push(Category {
                key: VIDEOS.into(),
                label: "Videos".into(),
                wifi_only: self.videos_wifi_only,
                folder: None,
            });
        }
        for f in self.folders.iter().filter(|f| f.enabled) {
            list.push(Category {
                key: f.category(),
                label: f.name.clone(),
                wifi_only: f.wifi_only,
                folder: Some(f.uri.clone()),
            });
        }
        list
    }
}

/// One thing the backup looks after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    /// The manifest's key and the first segment of the remote path.
    pub key: String,
    pub label: String,
    pub wifi_only: bool,
    /// The folder's tree, for a folder; `None` for the media library.
    pub folder: Option<String>,
}
