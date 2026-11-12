//! The index: the library tables in Postgres (the `songbook` schema, installed by Foundry).
//!
//! Every call checks a connection out of the pool. A [`Writer`] is a connection that may change the tables: taking one and
//! dropping it drop the cached copy of the library, so a snapshot built while a write was under way is never kept.

pub mod browse;
pub mod db;
pub mod follow;
pub mod songs;

pub use db::{Db, DbError, Result, Rows, Value};
use quench_pg::Pool;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// How `songs.key` is computed; stored in `meta` so a change of rule recomputes the keys once.
pub const KEY_SCHEME: &str = "unicode-1";

#[derive(Clone)]
pub struct Index {
    pool: Pool,
    /// The library as the page reads it; dropped by every write, rebuilt on the next `snapshot`.
    snapshot: Arc<Mutex<Option<Arc<browse::Snapshot>>>>,
    /// Counts the writes (twice each: when it starts and when it ends), for whatever keeps a copy of the tables.
    generation: Arc<AtomicU64>,
}

impl std::fmt::Debug for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Index { .. }")
    }
}

/// A connection that may change the tables; the cached copy of the library is dropped when it is taken and when it is dropped.
pub struct Writer {
    db: Db,
    index: Index,
}

impl std::fmt::Debug for Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Writer { .. }")
    }
}

impl Deref for Writer {
    type Target = Db;

    fn deref(&self) -> &Db {
        &self.db
    }
}

impl DerefMut for Writer {
    fn deref_mut(&mut self) -> &mut Db {
        &mut self.db
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.index.invalidate();
    }
}

impl Index {
    /// Uses the tables in `pool`'s schema and brings the song keys up to date. The schema is Foundry's: this creates nothing.
    pub async fn open(pool: Pool) -> Result<Self> {
        let index = Self {
            pool,
            snapshot: Arc::default(),
            generation: Arc::default(),
        };
        let mut db = index.write().await?;
        songs::rekey(&mut db).await?;
        drop(db);
        Ok(index)
    }

    /// The pool, for what keeps its own tables (the playlists).
    #[must_use]
    pub const fn pool(&self) -> &Pool {
        &self.pool
    }

    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }

    /// A connection that may change the tables.
    pub async fn write(&self) -> Result<Writer> {
        let db = Db::checkout(&self.pool).await?;
        self.invalidate();
        Ok(Writer {
            db,
            index: self.clone(),
        })
    }

    /// Runs `f` with a connection that may change the tables.
    pub async fn run<T>(&self, f: impl AsyncFnOnce(&mut Db) -> Result<T>) -> Result<T> {
        let mut writer = self.write().await?;
        f(&mut writer).await
    }

    /// A connection for reading: neither the snapshot nor the generation is touched.
    pub async fn read(&self) -> Result<Db> {
        Db::checkout(&self.pool).await
    }

    /// How many writes have started or ended: a copy of the tables made at another number is stale.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
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
        let started = self.generation();
        let mut db = self.read().await?;
        let built = Arc::new(browse::Snapshot::load(&mut db).await?);
        // Kept only if no write started or ended while it was being read.
        let mut slot = self.snapshot.lock().unwrap_or_else(PoisonError::into_inner);
        if self.generation() == started {
            *slot = Some(Arc::clone(&built));
        }
        Ok(built)
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

pub async fn meta_get(db: &mut Db, key: &str) -> Result<Option<String>> {
    let rows = db
        .query("SELECT value FROM meta WHERE key = ?", &[text(key)])
        .await?;
    Ok(rows.rows.first().map(|r| str_at(r, 0)))
}

pub async fn meta_set(db: &mut Db, key: &str, value: &str) -> Result<()> {
    db.execute("INSERT INTO meta(key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value", &[text(key), text(value)]).await?;
    Ok(())
}
