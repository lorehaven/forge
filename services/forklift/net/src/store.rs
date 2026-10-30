//! What the app keeps in its data directory: the session, the settings and the manifest. Files are written whole to a
//! temporary name and renamed into place, so a crash leaves the old content or the new, never half. The session file
//! can be read only by the user.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use forklift_core::{Manifest, Settings, Snapshot};

/// Numbers the manifest writes in the order they were asked for, and, for each data directory, the number of the last one that reached the disk:
/// a write that was overtaken (a later one is already there) is dropped, so a slow background write never puts an old
/// manifest over a newer one.
static ASKED: AtomicU64 = AtomicU64::new(0);
static WRITTEN: Mutex<BTreeMap<PathBuf, u64>> = Mutex::new(BTreeMap::new());

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

    /// The directory the platform gives apps: `$XDG_DATA_HOME/forklift`, else `$HOME/.local/share/forklift`. (On
    /// Android the host sets `HOME` to the app's files directory.)
    #[must_use]
    pub fn standard() -> Self {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(std::env::temp_dir);
        Self::new(base.join("forklift"))
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn write(&self, name: &str, text: &str, private: bool) {
        use std::os::unix::fs::OpenOptionsExt;
        let _ = std::fs::create_dir_all(&self.dir);
        let target = self.dir.join(name);
        let temp = self.dir.join(format!("{name}.tmp"));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        if private {
            options.mode(0o600);
        }
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

    /// The right to run the backup: one program at a time (the window or the background run) sends files and writes
    /// the manifest. `None` when another holds it. Released when the returned value is dropped, or when the program
    /// ends, however it ends.
    #[must_use]
    pub fn try_lock_run(&self) -> Option<RunLock> {
        let _ = std::fs::create_dir_all(&self.dir);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.dir.join("run.lock"))
            .ok()?;
        file.try_lock().ok()?;
        Some(RunLock(file))
    }

    /// Whether the host marked the window as being in the background (it writes a file called `hidden` there and
    /// removes it when the window is back).
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.dir.join("hidden").exists()
    }

    fn cursor_file(storage: &str) -> String {
        format!("sync-{}", forklift_core::sanitize(storage))
    }

    /// How far into a storage's change feed this device has read (0 for a storage never read).
    #[must_use]
    pub fn sync_cursor(&self, storage: &str) -> i64 {
        self.read(&Self::cursor_file(storage))
            .and_then(|t| t.trim().parse().ok())
            .unwrap_or(0)
    }

    pub fn set_sync_cursor(&self, storage: &str, id: i64) {
        self.write(&Self::cursor_file(storage), &id.to_string(), false);
    }

    /// The refresh token kept by an earlier run (or by the other program of this app: the background run).
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

    #[must_use]
    pub fn settings(&self) -> Settings {
        self.read("settings")
            .map(|t| Settings::from_text(&t))
            .unwrap_or_default()
    }

    pub fn set_settings(&self, settings: &Settings) {
        self.write("settings", &settings.to_text(), false);
    }

    /// The manifest, with files that were sending when the app died waiting again.
    #[must_use]
    pub fn manifest(&self) -> Manifest {
        let mut manifest = self
            .read("manifest")
            .map(|t| Manifest::from_text(&t))
            .unwrap_or_default();
        manifest.recover();
        manifest
    }

    pub fn set_manifest(&self, manifest: &Manifest) {
        self.write_manifest(manifest.to_text(), ASKED.fetch_add(1, Ordering::SeqCst) + 1);
    }

    /// [`set_manifest`](Self::set_manifest) without waiting for the disk (the text is made here, the writing, with its
    /// `fsync`, is another thread's): for the periodic save of a long run. A later [`set_manifest`](Self::set_manifest)
    /// is never overwritten by it.
    pub fn set_manifest_in_background(&self, manifest: &Manifest) {
        let (text, number) = (manifest.to_text(), ASKED.fetch_add(1, Ordering::SeqCst) + 1);
        let store = self.clone();
        std::thread::spawn(move || store.write_manifest(text, number));
    }

    fn write_manifest(&self, text: String, number: u64) {
        let mut written = WRITTEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let last = written.entry(self.dir.clone()).or_insert(0);
        if number > *last {
            *last = number;
            self.write("manifest", &text, false);
        }
    }

    /// What the dashboard showed last time: read before the manifest, so the screen has its numbers at once.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        self.read("snapshot")
            .map(|t| Snapshot::from_text(&t))
            .unwrap_or_default()
    }

    pub fn set_snapshot(&self, snapshot: &Snapshot) {
        self.write("snapshot", &snapshot.to_text(), false);
    }

    /// Signed out: what the screen showed goes with the session.
    pub fn clear_snapshot(&self) {
        let _ = std::fs::remove_file(self.dir.join("snapshot"));
    }

    /// When the backup last looked (epoch milliseconds).
    #[must_use]
    pub fn checked_ms(&self) -> Option<i64> {
        self.read("checked")?.trim().parse().ok()
    }

    pub fn set_checked_ms(&self, ms: i64) {
        self.write("checked", &ms.to_string(), false);
    }
}

/// Holds [`Store::try_lock_run`].
#[derive(Debug)]
pub struct RunLock(#[allow(dead_code, reason = "the lock is held by the open file")] std::fs::File);
