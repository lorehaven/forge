//! Playlists: each signed-in person's own, in a file of their own (`meta/user.db`) so that writing one never touches the
//! library index (whose cached copy every write to it drops).

use crate::index::{DbError, Result, Value, int, int_at, str_at, text};
use quench_sqlite::Connection;
use songbook_core::{PlaylistInfo, RecentItem, Resume, SongQuery};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS playlists(
  id INTEGER PRIMARY KEY, owner TEXT NOT NULL, name TEXT NOT NULL, created INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS playlist_songs(
  playlist_id INTEGER NOT NULL, position INTEGER NOT NULL, song_id INTEGER NOT NULL, PRIMARY KEY(playlist_id, position));
CREATE INDEX IF NOT EXISTS playlists_owner ON playlists(owner);
CREATE TABLE IF NOT EXISTS plays(
  owner TEXT NOT NULL, song_id INTEGER NOT NULL, at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS plays_owner_at ON plays(owner, at);
CREATE TABLE IF NOT EXISTS contexts(
  owner TEXT NOT NULL, key TEXT NOT NULL, kind TEXT NOT NULL, name TEXT NOT NULL, sub TEXT NOT NULL, art_kind TEXT NOT NULL,
  art_key TEXT NOT NULL, query TEXT NOT NULL, shuffle INTEGER NOT NULL, at INTEGER NOT NULL, PRIMARY KEY(owner, key));
CREATE INDEX IF NOT EXISTS contexts_owner_at ON contexts(owner, at);
CREATE TABLE IF NOT EXISTS resume(
  owner TEXT PRIMARY KEY, song_id INTEGER NOT NULL, position_ms INTEGER NOT NULL, queue TEXT NOT NULL,
  idx INTEGER NOT NULL, device TEXT NOT NULL, at INTEGER NOT NULL);";

/// How many plays of a person are kept (the oldest go).
pub const MAX_PLAYS: i64 = 500;

/// How many started things of a person are kept (the oldest go).
pub const MAX_CONTEXTS: i64 = 60;

/// At most this many entries in one playlist, and this many playlists for a person.
pub const MAX_SONGS: usize = 5000;
pub const MAX_PLAYLISTS: usize = 200;

#[derive(Clone)]
pub struct Playlists {
    db: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for Playlists {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Playlists { .. }")
    }
}

/// Why a change was not made.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// The playlist is not this person's, or does not exist.
    Unknown,
    /// Too many playlists or entries, or an empty name.
    Limit(&'static str),
}

impl Playlists {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DbError::Unsupported(format!("cannot create {}: {e}", parent.display()))
            })?;
        }
        let mut db = Connection::open_or_create(path)?;
        db.execute(SCHEMA, &[])?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }

    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        f(&mut self.db.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn owns(db: &mut Connection, owner: &str, id: i64) -> Result<bool> {
        Ok(!db
            .query(
                "SELECT id FROM playlists WHERE id = ? AND owner = ?",
                &[int(id), text(owner)],
            )?
            .rows
            .is_empty())
    }

    fn info(db: &mut Connection, id: i64) -> Result<Option<PlaylistInfo>> {
        let rows = db.query(
            "SELECT p.id, p.name, (SELECT COUNT(*) FROM playlist_songs s WHERE s.playlist_id = p.id) FROM playlists p WHERE p.id = ?",
            &[int(id)],
        )?;
        Ok(rows.rows.first().map(|r| row_info(r)))
    }

    /// Remembers that `owner` started `song` at `now`.
    pub fn played(&self, owner: &str, song: i64, now: i64) -> Result<()> {
        self.played_keeping(owner, song, now, MAX_PLAYS)
    }

    /// [`played`](Self::played), keeping `most` plays of this person.
    pub fn played_keeping(&self, owner: &str, song: i64, now: i64, most: i64) -> Result<()> {
        self.with(|db| {
            db.execute(
                "INSERT INTO plays(owner, song_id, at) VALUES (?, ?, ?)",
                &[text(owner), int(song), int(now)],
            )?;
            // Only the latest are kept.
            db.execute(
                "DELETE FROM plays WHERE owner = ? AND rowid NOT IN (SELECT rowid FROM plays WHERE owner = ? ORDER BY at DESC, rowid DESC LIMIT ?)",
                &[text(owner), text(owner), int(most)],
            )?;
            Ok(())
        })
    }

    /// Remembers that `owner` chose to start `item` at `now`: the same thing again moves to the front.
    pub fn started(&self, owner: &str, item: &RecentItem, now: i64) -> Result<()> {
        self.started_keeping(owner, item, now, MAX_CONTEXTS)
    }

    /// [`started`](Self::started), keeping `most` things of this person.
    pub fn started_keeping(
        &self,
        owner: &str,
        item: &RecentItem,
        now: i64,
        most: i64,
    ) -> Result<()> {
        let query = quench_serde::json::to_string(&item.query);
        self.with(|db| {
            db.execute(
                "INSERT INTO contexts(owner, key, kind, name, sub, art_kind, art_key, query, shuffle, at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(owner, key) DO UPDATE SET name = excluded.name, sub = excluded.sub, \
                 art_kind = excluded.art_kind, art_key = excluded.art_key, at = excluded.at",
                &[
                    text(owner),
                    text(&item.key()),
                    text(&item.kind),
                    text(&item.name),
                    text(&item.sub),
                    text(&item.art_kind),
                    text(&item.art_key),
                    text(&query),
                    int(i64::from(item.shuffle)),
                    int(now),
                ],
            )?;
            db.execute(
                "DELETE FROM contexts WHERE owner = ? AND key NOT IN (SELECT key FROM contexts WHERE owner = ? ORDER BY at DESC, rowid DESC LIMIT ?)",
                &[text(owner), text(owner), int(most)],
            )?;
            Ok(())
        })
    }

    /// What `owner` chose to start last, newest first.
    pub fn contexts(&self, owner: &str, limit: usize) -> Result<Vec<RecentItem>> {
        self.with(|db| {
            Ok(db
                .query(
                    "SELECT kind, name, sub, art_kind, art_key, query, shuffle, at FROM contexts WHERE owner = ? ORDER BY at DESC, rowid DESC LIMIT ?",
                    &[text(owner), int(i64::try_from(limit).unwrap_or(20))],
                )?
                .rows
                .iter()
                .map(|r| RecentItem {
                    kind: str_at(r, 0),
                    name: str_at(r, 1),
                    sub: str_at(r, 2),
                    art_kind: str_at(r, 3),
                    art_key: str_at(r, 4),
                    query: quench_serde::json::from_str::<SongQuery>(&str_at(r, 5))
                        .unwrap_or_default(),
                    shuffle: int_at(r, 6) != 0,
                    at: int_at(r, 7),
                })
                .collect())
        })
    }

    /// Remembers where `owner` stopped (replacing what was there), at `now`.
    pub fn set_resume(&self, owner: &str, r: &Resume, now: i64) -> Result<()> {
        let queue = r
            .queue
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.with(|db| {
            db.execute(
                "INSERT INTO resume(owner, song_id, position_ms, queue, idx, device, at) VALUES (?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(owner) DO UPDATE SET song_id = excluded.song_id, position_ms = excluded.position_ms, \
                 queue = excluded.queue, idx = excluded.idx, device = excluded.device, at = excluded.at",
                &[
                    text(owner),
                    int(r.song_id),
                    int(i64::try_from(r.position_ms).unwrap_or(i64::MAX)),
                    text(&queue),
                    int(r.index),
                    text(&r.device),
                    int(now),
                ],
            )?;
            Ok(())
        })
    }

    /// Where `owner` stopped last, if ever.
    pub fn resume(&self, owner: &str) -> Result<Option<Resume>> {
        self.with(|db| {
            Ok(db
                .query(
                    "SELECT song_id, position_ms, queue, idx, device, at FROM resume WHERE owner = ?",
                    &[text(owner)],
                )?
                .rows
                .first()
                .map(|r| Resume {
                    song_id: int_at(r, 0),
                    position_ms: u64::try_from(int_at(r, 1)).unwrap_or(0),
                    queue: str_at(r, 2)
                        .split(',')
                        .filter_map(|id| id.parse().ok())
                        .collect(),
                    index: int_at(r, 3),
                    device: str_at(r, 4),
                    at: int_at(r, 5),
                }))
        })
    }

    /// The songs `owner` played last, each once, newest first.
    pub fn recent(&self, owner: &str, limit: usize) -> Result<Vec<i64>> {
        self.with(|db| {
            Ok(db
                .query(
                    "SELECT song_id, MAX(at) AS last FROM plays WHERE owner = ? GROUP BY song_id ORDER BY last DESC, song_id DESC LIMIT ?",
                    &[text(owner), int(i64::try_from(limit).unwrap_or(40))],
                )?
                .rows
                .iter()
                .map(|r| int_at(r, 0))
                .collect())
        })
    }

    pub fn list(&self, owner: &str) -> Result<Vec<PlaylistInfo>> {
        self.with(|db| {
            Ok(db
                .query(
                    "SELECT p.id, p.name, (SELECT COUNT(*) FROM playlist_songs s WHERE s.playlist_id = p.id) \
                     FROM playlists p WHERE p.owner = ? ORDER BY lower(p.name), p.id",
                    &[text(owner)],
                )?
                .rows
                .iter()
                .map(|r| row_info(r))
                .collect())
        })
    }

    pub fn create(
        &self,
        owner: &str,
        name: &str,
        now: i64,
    ) -> Result<std::result::Result<PlaylistInfo, Refused>> {
        let name = clean(name);
        if name.is_empty() {
            return Ok(Err(Refused::Limit("A playlist needs a name.")));
        }
        self.with(|db| {
            if self_count(db, owner)? >= MAX_PLAYLISTS {
                return Ok(Err(Refused::Limit("Too many playlists.")));
            }
            db.execute(
                "INSERT INTO playlists(owner, name, created) VALUES (?, ?, ?)",
                &[text(owner), text(&name), int(now)],
            )?;
            let id = db.last_insert_rowid();
            Ok(Ok(Self::info(db, id)?.expect("the playlist just made")))
        })
    }

    pub fn rename(
        &self,
        owner: &str,
        id: i64,
        name: &str,
    ) -> Result<std::result::Result<PlaylistInfo, Refused>> {
        let name = clean(name);
        if name.is_empty() {
            return Ok(Err(Refused::Limit("A playlist needs a name.")));
        }
        self.with(|db| {
            if !Self::owns(db, owner, id)? {
                return Ok(Err(Refused::Unknown));
            }
            db.execute(
                "UPDATE playlists SET name = ? WHERE id = ?",
                &[text(&name), int(id)],
            )?;
            Ok(Ok(Self::info(db, id)?.expect("the playlist just renamed")))
        })
    }

    pub fn delete(&self, owner: &str, id: i64) -> Result<bool> {
        self.with(|db| {
            if !Self::owns(db, owner, id)? {
                return Ok(false);
            }
            db.execute(
                "DELETE FROM playlist_songs WHERE playlist_id = ?",
                &[int(id)],
            )?;
            db.execute("DELETE FROM playlists WHERE id = ?", &[int(id)])?;
            Ok(true)
        })
    }

    /// The song ids of a playlist in order; `None` for one that is not this person's.
    pub fn songs(&self, owner: &str, id: i64) -> Result<Option<Vec<i64>>> {
        self.with(|db| {
            if !Self::owns(db, owner, id)? {
                return Ok(None);
            }
            Ok(Some(
                db.query(
                    "SELECT song_id FROM playlist_songs WHERE playlist_id = ? ORDER BY position",
                    &[int(id)],
                )?
                .rows
                .iter()
                .map(|r| int_at(r, 0))
                .collect(),
            ))
        })
    }

    /// Adds songs at the end.
    pub fn add(
        &self,
        owner: &str,
        id: i64,
        songs: &[i64],
    ) -> Result<std::result::Result<PlaylistInfo, Refused>> {
        self.with(|db| {
            if !Self::owns(db, owner, id)? {
                return Ok(Err(Refused::Unknown));
            }
            let have = usize::try_from(int_at(
                &db.query(
                    "SELECT COUNT(*) FROM playlist_songs WHERE playlist_id = ?",
                    &[int(id)],
                )?
                .rows[0],
                0,
            ))
            .unwrap_or(0);
            if have + songs.len() > MAX_SONGS {
                return Ok(Err(Refused::Limit("The playlist is full.")));
            }
            let last = db
                .query(
                    "SELECT COALESCE(MAX(position), -1) FROM playlist_songs WHERE playlist_id = ?",
                    &[int(id)],
                )?
                .rows[0]
                .first()
                .and_then(Value::as_integer)
                .unwrap_or(-1);
            db.execute("BEGIN", &[])?;
            for (n, song) in songs.iter().enumerate() {
                let position = last + 1 + i64::try_from(n).unwrap_or(0);
                if let Err(e) = db.execute(
                    "INSERT INTO playlist_songs(playlist_id, position, song_id) VALUES (?, ?, ?)",
                    &[int(id), int(position), int(*song)],
                ) {
                    let _ = db.execute("ROLLBACK", &[]);
                    return Err(e);
                }
            }
            db.execute("COMMIT", &[])?;
            Ok(Ok(Self::info(db, id)?.expect("the playlist")))
        })
    }

    /// Replaces the entries (to remove, to move, to clear).
    pub fn set(
        &self,
        owner: &str,
        id: i64,
        songs: &[i64],
    ) -> Result<std::result::Result<PlaylistInfo, Refused>> {
        if songs.len() > MAX_SONGS {
            return Ok(Err(Refused::Limit("The playlist is full.")));
        }
        self.with(|db| {
            if !Self::owns(db, owner, id)? {
                return Ok(Err(Refused::Unknown));
            }
            db.execute("BEGIN", &[])?;
            let done = (|| -> Result<()> {
                db.execute("DELETE FROM playlist_songs WHERE playlist_id = ?", &[int(id)])?;
                for (n, song) in songs.iter().enumerate() {
                    db.execute(
                        "INSERT INTO playlist_songs(playlist_id, position, song_id) VALUES (?, ?, ?)",
                        &[int(id), int(i64::try_from(n).unwrap_or(0)), int(*song)],
                    )?;
                }
                Ok(())
            })();
            if let Err(e) = done {
                let _ = db.execute("ROLLBACK", &[]);
                return Err(e);
            }
            db.execute("COMMIT", &[])?;
            Ok(Ok(Self::info(db, id)?.expect("the playlist")))
        })
    }
}

fn row_info(r: &[Value]) -> PlaylistInfo {
    PlaylistInfo {
        id: int_at(r, 0),
        name: str_at(r, 1),
        songs: u32::try_from(int_at(r, 2)).unwrap_or(0),
    }
}

fn self_count(db: &mut Connection, owner: &str) -> Result<usize> {
    let rows = db.query(
        "SELECT COUNT(*) FROM playlists WHERE owner = ?",
        &[text(owner)],
    )?;
    Ok(usize::try_from(int_at(&rows.rows[0], 0)).unwrap_or(0))
}

/// A name as kept: trimmed, one line, at most 100 characters.
fn clean(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(100)
        .collect()
}
