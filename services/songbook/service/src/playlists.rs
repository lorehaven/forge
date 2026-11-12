//! Playlists: each signed-in person's own, with their plays, the things they started and where they stopped. They live in
//! tables of their own, apart from the library, so writing one never drops the cached copy of the library.

use crate::index::{Db, Result, Value, int, int_at, str_at, text};
use quench_pg::Pool;
use songbook_core::{PlaylistInfo, RecentItem, Resume, SongQuery};

/// How many plays of a person are kept (the oldest go).
pub const MAX_PLAYS: i64 = 500;

/// How many started things of a person are kept (the oldest go).
pub const MAX_CONTEXTS: i64 = 60;

/// At most this many entries in one playlist, and this many playlists for a person.
pub const MAX_SONGS: usize = 5000;
pub const MAX_PLAYLISTS: usize = 200;

#[derive(Clone)]
pub struct Playlists {
    pool: Pool,
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
    #[must_use]
    pub const fn new(pool: Pool) -> Self {
        Self { pool }
    }

    async fn db(&self) -> Result<Db> {
        Db::checkout(&self.pool).await
    }

    async fn owns(db: &mut Db, owner: &str, id: i64) -> Result<bool> {
        Ok(!db
            .query(
                "SELECT id FROM playlists WHERE id = ? AND owner = ?",
                &[int(id), text(owner)],
            )
            .await?
            .rows
            .is_empty())
    }

    async fn info(db: &mut Db, id: i64) -> Result<Option<PlaylistInfo>> {
        let rows = db
            .query(
                "SELECT p.id, p.name, (SELECT COUNT(*) FROM playlist_songs s WHERE s.playlist_id = p.id) FROM playlists p WHERE p.id = ?",
                &[int(id)],
            )
            .await?;
        Ok(rows.rows.first().map(|r| row_info(r)))
    }

    /// Remembers that `owner` started `song` at `now`.
    pub async fn played(&self, owner: &str, song: i64, now: i64) -> Result<()> {
        self.played_keeping(owner, song, now, MAX_PLAYS).await
    }

    /// [`played`](Self::played), keeping `most` plays of this person.
    pub async fn played_keeping(&self, owner: &str, song: i64, now: i64, most: i64) -> Result<()> {
        let mut db = self.db().await?;
        db.execute(
            "INSERT INTO plays(owner, song_id, at) VALUES (?, ?, ?)",
            &[text(owner), int(song), int(now)],
        )
        .await?;
        // Only the latest are kept.
        db.execute(
            "DELETE FROM plays WHERE owner = ? AND id NOT IN (SELECT id FROM plays WHERE owner = ? ORDER BY at DESC, id DESC LIMIT ?)",
            &[text(owner), text(owner), int(most)],
        )
        .await?;
        Ok(())
    }

    /// Remembers that `owner` chose to start `item` at `now`: the same thing again moves to the front.
    pub async fn started(&self, owner: &str, item: &RecentItem, now: i64) -> Result<()> {
        self.started_keeping(owner, item, now, MAX_CONTEXTS).await
    }

    /// [`started`](Self::started), keeping `most` things of this person.
    pub async fn started_keeping(
        &self,
        owner: &str,
        item: &RecentItem,
        now: i64,
        most: i64,
    ) -> Result<()> {
        let query = quench_serde::json::to_string(&item.query);
        let mut db = self.db().await?;
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
        )
        .await?;
        db.execute(
            "DELETE FROM contexts WHERE owner = ? AND key NOT IN (SELECT key FROM contexts WHERE owner = ? ORDER BY at DESC, key DESC LIMIT ?)",
            &[text(owner), text(owner), int(most)],
        )
        .await?;
        Ok(())
    }

    /// What `owner` chose to start last, newest first.
    pub async fn contexts(&self, owner: &str, limit: usize) -> Result<Vec<RecentItem>> {
        let mut db = self.db().await?;
        Ok(db
            .query(
                "SELECT kind, name, sub, art_kind, art_key, query, shuffle, at FROM contexts WHERE owner = ? ORDER BY at DESC, key DESC LIMIT ?",
                &[text(owner), int(i64::try_from(limit).unwrap_or(20))],
            )
            .await?
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
    }

    /// Remembers where `owner` stopped (replacing what was there), at `now`.
    pub async fn set_resume(&self, owner: &str, r: &Resume, now: i64) -> Result<()> {
        let queue = r
            .queue
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let mut db = self.db().await?;
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
        )
        .await?;
        Ok(())
    }

    /// Where `owner` stopped last, if ever.
    pub async fn resume(&self, owner: &str) -> Result<Option<Resume>> {
        let mut db = self.db().await?;
        Ok(db
            .query(
                "SELECT song_id, position_ms, queue, idx, device, at FROM resume WHERE owner = ?",
                &[text(owner)],
            )
            .await?
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
    }

    /// The songs `owner` played last, each once, newest first.
    pub async fn recent(&self, owner: &str, limit: usize) -> Result<Vec<i64>> {
        let mut db = self.db().await?;
        Ok(db
            .query(
                "SELECT song_id, MAX(at) AS latest FROM plays WHERE owner = ? GROUP BY song_id ORDER BY latest DESC, song_id DESC LIMIT ?",
                &[text(owner), int(i64::try_from(limit).unwrap_or(40))],
            )
            .await?
            .rows
            .iter()
            .map(|r| int_at(r, 0))
            .collect())
    }

    pub async fn list(&self, owner: &str) -> Result<Vec<PlaylistInfo>> {
        let mut db = self.db().await?;
        Ok(db
            .query(
                "SELECT p.id, p.name, (SELECT COUNT(*) FROM playlist_songs s WHERE s.playlist_id = p.id) \
                 FROM playlists p WHERE p.owner = ? ORDER BY lower(p.name) COLLATE \"C\", p.id",
                &[text(owner)],
            )
            .await?
            .rows
            .iter()
            .map(|r| row_info(r))
            .collect())
    }

    pub async fn create(
        &self,
        owner: &str,
        name: &str,
        now: i64,
    ) -> Result<std::result::Result<PlaylistInfo, Refused>> {
        let name = clean(name);
        if name.is_empty() {
            return Ok(Err(Refused::Limit("A playlist needs a name.")));
        }
        let mut db = self.db().await?;
        if self_count(&mut db, owner).await? >= MAX_PLAYLISTS {
            return Ok(Err(Refused::Limit("Too many playlists.")));
        }
        let id = db
            .insert(
                "INSERT INTO playlists(owner, name, created) VALUES (?, ?, ?)",
                &[text(owner), text(&name), int(now)],
            )
            .await?;
        Ok(Ok(Self::info(&mut db, id)
            .await?
            .expect("the playlist just made")))
    }

    pub async fn rename(
        &self,
        owner: &str,
        id: i64,
        name: &str,
    ) -> Result<std::result::Result<PlaylistInfo, Refused>> {
        let name = clean(name);
        if name.is_empty() {
            return Ok(Err(Refused::Limit("A playlist needs a name.")));
        }
        let mut db = self.db().await?;
        if !Self::owns(&mut db, owner, id).await? {
            return Ok(Err(Refused::Unknown));
        }
        db.execute(
            "UPDATE playlists SET name = ? WHERE id = ?",
            &[text(&name), int(id)],
        )
        .await?;
        Ok(Ok(Self::info(&mut db, id)
            .await?
            .expect("the playlist just renamed")))
    }

    pub async fn delete(&self, owner: &str, id: i64) -> Result<bool> {
        let mut db = self.db().await?;
        if !Self::owns(&mut db, owner, id).await? {
            return Ok(false);
        }
        db.transaction(async |db| {
            db.execute(
                "DELETE FROM playlist_songs WHERE playlist_id = ?",
                &[int(id)],
            )
            .await?;
            db.execute("DELETE FROM playlists WHERE id = ?", &[int(id)])
                .await?;
            Ok(())
        })
        .await?;
        Ok(true)
    }

    /// The song ids of a playlist in order; `None` for one that is not this person's.
    pub async fn songs(&self, owner: &str, id: i64) -> Result<Option<Vec<i64>>> {
        let mut db = self.db().await?;
        if !Self::owns(&mut db, owner, id).await? {
            return Ok(None);
        }
        Ok(Some(
            db.query(
                "SELECT song_id FROM playlist_songs WHERE playlist_id = ? ORDER BY position",
                &[int(id)],
            )
            .await?
            .rows
            .iter()
            .map(|r| int_at(r, 0))
            .collect(),
        ))
    }

    /// Adds songs at the end.
    pub async fn add(
        &self,
        owner: &str,
        id: i64,
        songs: &[i64],
    ) -> Result<std::result::Result<PlaylistInfo, Refused>> {
        let mut db = self.db().await?;
        if !Self::owns(&mut db, owner, id).await? {
            return Ok(Err(Refused::Unknown));
        }
        let have = usize::try_from(int_at(
            &db.query(
                "SELECT COUNT(*) FROM playlist_songs WHERE playlist_id = ?",
                &[int(id)],
            )
            .await?
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
            )
            .await?
            .rows[0]
            .first()
            .and_then(Value::as_integer)
            .unwrap_or(-1);
        db.transaction(async |db| {
            for (n, song) in songs.iter().enumerate() {
                let position = last + 1 + i64::try_from(n).unwrap_or(0);
                db.execute(
                    "INSERT INTO playlist_songs(playlist_id, position, song_id) VALUES (?, ?, ?)",
                    &[int(id), int(position), int(*song)],
                )
                .await?;
            }
            Ok(())
        })
        .await?;
        Ok(Ok(Self::info(&mut db, id).await?.expect("the playlist")))
    }

    /// Replaces the entries (to remove, to move, to clear).
    pub async fn set(
        &self,
        owner: &str,
        id: i64,
        songs: &[i64],
    ) -> Result<std::result::Result<PlaylistInfo, Refused>> {
        if songs.len() > MAX_SONGS {
            return Ok(Err(Refused::Limit("The playlist is full.")));
        }
        let mut db = self.db().await?;
        if !Self::owns(&mut db, owner, id).await? {
            return Ok(Err(Refused::Unknown));
        }
        db.transaction(async |db| {
            db.execute(
                "DELETE FROM playlist_songs WHERE playlist_id = ?",
                &[int(id)],
            )
            .await?;
            for (n, song) in songs.iter().enumerate() {
                db.execute(
                    "INSERT INTO playlist_songs(playlist_id, position, song_id) VALUES (?, ?, ?)",
                    &[int(id), int(i64::try_from(n).unwrap_or(0)), int(*song)],
                )
                .await?;
            }
            Ok(())
        })
        .await?;
        Ok(Ok(Self::info(&mut db, id).await?.expect("the playlist")))
    }
}

fn row_info(r: &[Value]) -> PlaylistInfo {
    PlaylistInfo {
        id: int_at(r, 0),
        name: str_at(r, 1),
        songs: u32::try_from(int_at(r, 2)).unwrap_or(0),
    }
}

async fn self_count(db: &mut Db, owner: &str) -> Result<usize> {
    let rows = db
        .query(
            "SELECT COUNT(*) FROM playlists WHERE owner = ?",
            &[text(owner)],
        )
        .await?;
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
