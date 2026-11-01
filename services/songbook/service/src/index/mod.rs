//! The index: `meta/library.db`, opened through `quench-sqlite`.
//!
//! One connection behind a mutex (the service is the only writer; the engine locks the file exclusively anyway). Calls are
//! blocking and short; the long ones (a sync's write phase) run inside `spawn_blocking` at their call site.

pub mod browse;
pub mod follow;
pub mod import;
pub mod songs;

use quench_sqlite::Connection;
pub use quench_sqlite::{Error as DbError, Value};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

pub type Result<T> = std::result::Result<T, DbError>;

/// The tables, run on every start (`CREATE ... IF NOT EXISTS`). The same file builds a converted database.
pub const SCHEMA: &str = include_str!("../schema.sql");

/// How `songs.key` is computed; stored in `meta` so a change of rule recomputes the keys once.
pub const KEY_SCHEME: &str = "unicode-1";

#[derive(Clone)]
pub struct Index {
    db: Arc<Mutex<Connection>>,
    /// The library as the page reads it; dropped by every call that is not a read of it, rebuilt on the next `snapshot`.
    snapshot: Arc<Mutex<Option<Arc<browse::Snapshot>>>>,
}

impl std::fmt::Debug for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Index { .. }")
    }
}

impl Index {
    /// Opens (creating, with the schema) the database at `path`, brings its keys up to date and, on a first start with an
    /// empty `follows` table, imports the text files of the Python helper from `meta_dir`.
    pub fn open(path: &Path, meta_dir: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DbError::Unsupported(format!("cannot create {}: {e}", parent.display()))
            })?;
        }
        let mut db = Connection::open_or_create(path)?;
        db.execute(SCHEMA, &[])?;
        let index = Self {
            db: Arc::new(Mutex::new(db)),
            snapshot: Arc::default(),
        };
        index.with(|db| {
            songs::rekey(db)?;
            import::once(db, meta_dir)
        })?;
        Ok(index)
    }

    /// Runs `f` with the connection. Anything may change under it, so the snapshot of the library is dropped.
    pub fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut guard = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        let out = f(&mut guard);
        // Taken while the connection is still held, so a snapshot built from the old tables cannot outlive the change.
        self.snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        out
    }

    /// The library as the page reads it: built from the tables on the first call after a change, then shared.
    pub async fn snapshot(&self) -> Result<Arc<browse::Snapshot>> {
        let cached = self
            .snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(ready) = cached {
            return Ok(ready);
        }
        let index = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = index.db.lock().unwrap_or_else(PoisonError::into_inner);
            let built = Arc::new(browse::Snapshot::load(&mut guard)?);
            // Stored while the connection is still held: no write can slip between the read and the store.
            *index
                .snapshot
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&built));
            Ok(built)
        })
        .await
        .unwrap_or_else(|e| {
            Err(DbError::Unsupported(format!(
                "the database task failed: {e}"
            )))
        })
    }

    /// `with` on a blocking thread, for anything that touches many rows.
    pub async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let index = self.clone();
        tokio::task::spawn_blocking(move || index.with(f))
            .await
            .unwrap_or_else(|e| {
                Err(DbError::Unsupported(format!(
                    "the database task failed: {e}"
                )))
            })
    }
}

#[must_use]
pub const fn int(v: i64) -> Value {
    Value::Integer(v)
}

#[must_use]
pub fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

/// A text cell; empty for NULL.
#[must_use]
pub fn str_at(row: &[Value], i: usize) -> String {
    row.get(i)
        .and_then(Value::as_text)
        .unwrap_or_default()
        .to_string()
}

/// An integer cell; 0 for NULL.
#[must_use]
pub fn int_at(row: &[Value], i: usize) -> i64 {
    row.get(i).and_then(Value::as_integer).unwrap_or(0)
}

/// An integer cell; `None` for NULL.
#[must_use]
pub fn opt_int_at(row: &[Value], i: usize) -> Option<i64> {
    row.get(i).and_then(Value::as_integer)
}

/// A text cell; `None` for NULL.
#[must_use]
pub fn opt_str_at(row: &[Value], i: usize) -> Option<String> {
    row.get(i).and_then(Value::as_text).map(str::to_string)
}

/// `?,?,?` for `n` parameters.
#[must_use]
pub fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

pub fn meta_get(db: &mut Connection, key: &str) -> Result<Option<String>> {
    let rows = db.query("SELECT value FROM meta WHERE key = ?", &[text(key)])?;
    Ok(rows.rows.first().map(|r| str_at(r, 0)))
}

pub fn meta_set(db: &mut Connection, key: &str, value: &str) -> Result<()> {
    db.execute("INSERT INTO meta(key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value", &[text(key), text(value)])?;
    Ok(())
}
