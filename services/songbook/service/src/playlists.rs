//! Playlists: each signed-in person's own, in a file of their own (`meta/user.db`) so that writing one never touches the
//! library index (whose cached copy every write to it drops).

use crate::index::{DbError, Result, Value, int, int_at, str_at, text};
use quench_sqlite::Connection;
use songbook_core::PlaylistInfo;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS playlists(
  id INTEGER PRIMARY KEY, owner TEXT NOT NULL, name TEXT NOT NULL, created INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS playlist_songs(
  playlist_id INTEGER NOT NULL, position INTEGER NOT NULL, song_id INTEGER NOT NULL, PRIMARY KEY(playlist_id, position));
CREATE INDEX IF NOT EXISTS playlists_owner ON playlists(owner);";

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
