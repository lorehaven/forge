//! What the app keeps in its data directory: the session, the settings and the manifest. The small files are written
//! whole to a temporary name and renamed into place, so a crash leaves the old content or the new, never half; the
//! session file can be read only by the user. The manifest is a SQLite database (`manifest.db`, made by
//! `quench-sqlite`): a save writes only the records that changed, in one transaction, so a crash leaves the old
//! manifest or the new one and a big library is not rewritten for a few changed files.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use forklift_core::{Item, Manifest, Settings, Snapshot, Status};
use quench_sqlite::{Connection, Value};

/// Numbers the manifest writes in the order they were asked for, and, for each data directory, the number of the last one that reached the disk:
/// a write that was overtaken (a later one is already there) is dropped, so a slow background write never puts an old
/// manifest over a newer one.
static ASKED: AtomicU64 = AtomicU64::new(0);
static WRITTEN: Mutex<BTreeMap<PathBuf, u64>> = Mutex::new(BTreeMap::new());
/// Background writes not finished yet, by data directory: a read of the manifest waits for them, so it never sees a
/// manifest older than the last one this program asked to be written.
static PENDING: Mutex<BTreeMap<PathBuf, usize>> = Mutex::new(BTreeMap::new());
static WRITTEN_SIGNAL: Condvar = Condvar::new();
/// Data directories whose manifest could not be read the last time: nothing is written there until it has been, so a
/// manifest that is empty only because the database was busy never replaces the real one.
static UNREADABLE: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// The manifest's database file, in the data directory.
const MANIFEST_DB: &str = "manifest.db";
/// How many records wait, by status, as three lines of text: what the Android host (Kotlin) reads to decide whether to
/// start the program and what the status notification says, without opening the database.
const COUNTS: &str = "manifest.counts";
/// The manifest of earlier versions: one text line per record. Read once into the database, then renamed.
const LEGACY_MANIFEST: &str = "manifest";

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS items (
    category TEXT NOT NULL, local_id TEXT NOT NULL, name TEXT NOT NULL, uri TEXT NOT NULL,
    size INTEGER NOT NULL, modified_ms INTEGER NOT NULL, status TEXT NOT NULL, attempts INTEGER NOT NULL,
    retry_at_ms INTEGER NOT NULL, error TEXT NOT NULL, remote TEXT NOT NULL, gone INTEGER NOT NULL,
    PRIMARY KEY (category, local_id));
CREATE INDEX IF NOT EXISTS items_status ON items (status, retry_at_ms)";

const COLUMNS: &str = "category, local_id, name, uri, size, modified_ms, status, attempts, retry_at_ms, error, remote, gone";

const fn status_word(status: Status) -> &'static str {
    match status {
        Status::Pending => "pending",
        Status::Uploading => "uploading",
        Status::Done => "done",
        Status::Failed => "failed",
        Status::TooLarge => "too_large",
    }
}

fn parse_status(word: &str) -> Option<Status> {
    Some(match word {
        "pending" => Status::Pending,
        "uploading" => Status::Uploading,
        "done" => Status::Done,
        "failed" => Status::Failed,
        "too_large" => Status::TooLarge,
        _ => return None,
    })
}

fn item_values(i: &Item) -> Vec<Value> {
    vec![
        Value::Text(i.category.clone()),
        Value::Text(i.local_id.clone()),
        Value::Text(i.name.clone()),
        Value::Text(i.uri.clone()),
        Value::Integer(i64::try_from(i.size).unwrap_or(i64::MAX)),
        Value::Integer(i.modified_ms),
        Value::Text(status_word(i.status).to_string()),
        Value::Integer(i64::from(i.attempts)),
        Value::Integer(i.retry_at_ms),
        Value::Text(i.error.clone()),
        Value::Text(i.remote.clone()),
        Value::Integer(i64::from(i.gone)),
    ]
}

fn item_from_row(row: &[Value]) -> Option<Item> {
    let text = |i: usize| row.get(i).and_then(Value::as_text).map(str::to_string);
    let int = |i: usize| row.get(i).and_then(Value::as_integer);
    Some(Item {
        category: text(0)?,
        local_id: text(1)?,
        name: text(2)?,
        uri: text(3)?,
        size: u64::try_from(int(4)?).ok()?,
        modified_ms: int(5)?,
        status: parse_status(&text(6)?)?,
        attempts: u32::try_from(int(7)?).ok()?,
        retry_at_ms: int(8)?,
        error: text(9)?,
        remote: text(10)?,
        gone: int(11)? != 0,
    })
}

fn read_items(conn: &mut Connection) -> Option<Vec<Item>> {
    let rows = conn
        .query(&format!("SELECT {COLUMNS} FROM items"), &[])
        .ok()?;
    // A record that does not read back means damage; losing it only means the next scan looks at the file again.
    Some(rows.rows.iter().filter_map(|r| item_from_row(r)).collect())
}

/// Makes the database hold exactly `items`, writing only what differs, in one transaction.
fn persist(conn: &mut Connection, items: &[Item]) -> quench_sqlite::Result<()> {
    conn.transaction(|conn| {
        let mut stored: HashMap<(String, String), Item> = read_items(conn)
            .unwrap_or_default()
            .into_iter()
            .map(|i| ((i.category.clone(), i.local_id.clone()), i))
            .collect();
        let insert = format!(
            "INSERT OR REPLACE INTO items ({COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        );
        for item in items {
            let key = (item.category.clone(), item.local_id.clone());
            if stored.remove(&key).as_ref() != Some(item) {
                conn.execute(&insert, &item_values(item))?;
            }
        }
        // What is left was in the database and is not in the manifest any more.
        for (category, local_id) in stored.into_keys() {
            conn.execute(
                "DELETE FROM items WHERE category = ? AND local_id = ?",
                &[Value::Text(category), Value::Text(local_id)],
            )?;
        }
        Ok(())
    })
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

    /// The manifest database, made if it is not there. A writer in another program holds it for the length of one
    /// commit, so a busy answer is waited out for a while.
    fn open_db(&self) -> Option<Connection> {
        let _ = std::fs::create_dir_all(&self.dir);
        for _ in 0..150 {
            match Connection::open_or_create(self.dir.join(MANIFEST_DB)) {
                Ok(mut conn) => return conn.execute(SCHEMA, &[]).ok().map(|_| conn),
                Err(quench_sqlite::Error::Busy) => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => return None,
            }
        }
        None
    }

    /// Writes [`COUNTS`] for these records.
    fn write_counts(&self, items: &[Item]) {
        let count = |status: Status| items.iter().filter(|i| i.status == status).count();
        let text = format!(
            "forklift-counts 1\npending {}\nuploading {}\nfailed {}\n",
            count(Status::Pending),
            count(Status::Uploading),
            count(Status::Failed)
        );
        self.write(COUNTS, &text, false);
    }

    /// Reads the records of the earlier text manifest into an empty database, once.
    fn import_legacy(&self, conn: &mut Connection) {
        let legacy = self.dir.join(LEGACY_MANIFEST);
        let empty = conn
            .query("SELECT count(*) FROM items", &[])
            .ok()
            .and_then(|r| {
                r.rows
                    .first()
                    .and_then(|row| row.first().and_then(Value::as_integer))
            })
            == Some(0);
        if !empty {
            return;
        }
        let Ok(text) = std::fs::read_to_string(&legacy) else {
            return;
        };
        let items: Vec<Item> = Manifest::from_text(&text).items().cloned().collect();
        if persist(conn, &items).is_ok() {
            self.write_counts(&items);
            let _ = std::fs::rename(&legacy, self.dir.join("manifest.imported"));
        }
    }

    /// The manifest, with files that were sending when the app died waiting again.
    #[must_use]
    pub fn manifest(&self) -> Manifest {
        self.settle_writes();
        let items = self.open_db().and_then(|mut conn| {
            self.import_legacy(&mut conn);
            read_items(&mut conn)
        });
        let mut unreadable = UNREADABLE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut manifest = if let Some(items) = items {
            unreadable.remove(&self.dir);
            Manifest::from_items(items)
        } else {
            unreadable.insert(self.dir.clone());
            Manifest::default()
        };
        drop(unreadable);
        manifest.recover();
        manifest
    }

    pub fn set_manifest(&self, manifest: &Manifest) {
        let items = manifest.items().cloned().collect();
        self.write_manifest(items, ASKED.fetch_add(1, Ordering::SeqCst) + 1);
    }

    /// [`set_manifest`](Self::set_manifest) without waiting for the disk (the records are copied here, the writing,
    /// with its `fsync`, is another thread's): for the periodic save of a long run. A later
    /// [`set_manifest`](Self::set_manifest) is never overwritten by it.
    pub fn set_manifest_in_background(&self, manifest: &Manifest) {
        let (items, number): (Vec<Item>, u64) = (
            manifest.items().cloned().collect(),
            ASKED.fetch_add(1, Ordering::SeqCst) + 1,
        );
        let store = self.clone();
        *PENDING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(self.dir.clone())
            .or_insert(0) += 1;
        std::thread::spawn(move || {
            store.write_manifest(items, number);
            let mut pending = PENDING
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(n) = pending.get_mut(&store.dir) {
                *n = n.saturating_sub(1);
            }
            WRITTEN_SIGNAL.notify_all();
        });
    }

    /// Waits until the manifest writes asked for in the background have reached the disk.
    fn settle_writes(&self) {
        let mut pending = PENDING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while pending.get(&self.dir).copied().unwrap_or(0) > 0 {
            pending = WRITTEN_SIGNAL
                .wait(pending)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn write_manifest(&self, items: Vec<Item>, number: u64) {
        let mut written = WRITTEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let last = written.entry(self.dir.clone()).or_insert(0);
        if number <= *last {
            return;
        }
        *last = number;
        let unreadable = UNREADABLE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&self.dir);
        if unreadable {
            return;
        }
        if let Some(mut conn) = self.open_db()
            && persist(&mut conn, &items).is_ok()
        {
            self.write_counts(&items);
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
