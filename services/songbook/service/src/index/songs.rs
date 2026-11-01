//! Writing the catalogue: artists, albums, songs and how they link, plus the derived tags.

use super::{
    KEY_SCHEME, Result, Value, int, int_at, meta_get, meta_set, opt_int_at, placeholders, str_at,
    text,
};
use quench_sqlite::Connection;
use songbook_core::naming::{clean_title, disambiguate, make_filename, song_key};
use std::collections::{BTreeSet, HashMap, HashSet};

/// The name shown for artists with no category.
pub const UNCATEGORIZED: &str = "Uncategorized";

/// Everything Deezer says about an album that the index keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumInfo {
    pub deezer_id: i64,
    pub title: String,
    pub release_date: String,
    pub record_type: String,
    pub genres: Vec<String>,
}

/// Deezer uses `0000-00-00` for unknown dates; anything before 1900 is unknown.
#[must_use]
pub fn year_of(date: &str) -> String {
    let y: String = date.chars().take(4).collect();
    match y.parse::<u32>() {
        Ok(n) if y.len() == 4 && n >= 1900 => y,
        _ => String::new(),
    }
}

/// The artist with this Deezer id, created if new. `listed_as` (the name a follow was entered under) moves to this row.
pub fn upsert_artist(
    db: &mut Connection,
    deezer_id: i64,
    name: &str,
    listed_as: Option<&str>,
) -> Result<i64> {
    let found = db.query(
        "SELECT id FROM artists WHERE deezer_id = ?",
        &[int(deezer_id)],
    )?;
    let id = if let Some(row) = found.rows.first() {
        let id = int_at(row, 0);
        if let Some(listed) = listed_as {
            db.execute(
                "UPDATE artists SET listed_as = NULL WHERE listed_as = ? AND id != ?",
                &[text(listed), int(id)],
            )?;
            db.execute(
                "UPDATE artists SET name = ?, listed_as = ? WHERE id = ?",
                &[text(name), text(listed), int(id)],
            )?;
        } else {
            db.execute(
                "UPDATE artists SET name = ? WHERE id = ?",
                &[text(name), int(id)],
            )?;
        }
        id
    } else {
        if let Some(listed) = listed_as {
            db.execute(
                "UPDATE artists SET listed_as = NULL WHERE listed_as = ?",
                &[text(listed)],
            )?;
        }
        db.execute(
            "INSERT INTO artists(deezer_id, name, listed_as) VALUES (?, ?, ?)",
            &[
                int(deezer_id),
                text(name),
                listed_as.map_or(Value::Null, text),
            ],
        )?;
        db.last_insert_rowid()
    };
    Ok(id)
}

pub fn upsert_album(db: &mut Connection, album: &AlbumInfo, owner: i64) -> Result<i64> {
    let genre = if album.genres.is_empty() {
        "Unknown".to_string()
    } else {
        album.genres.join("; ")
    };
    let values = [
        text(&album.title),
        text(&year_of(&album.release_date)),
        text(&album.record_type),
        text(&genre),
        int(owner),
    ];
    let found = db.query(
        "SELECT id FROM albums WHERE deezer_id = ?",
        &[int(album.deezer_id)],
    )?;
    if let Some(row) = found.rows.first() {
        let id = int_at(row, 0);
        let mut params = values.to_vec();
        params.push(int(id));
        db.execute(
            "UPDATE albums SET title = ?, year = ?, type = ?, genre = ?, owner_id = ? WHERE id = ?",
            &params,
        )?;
        Ok(id)
    } else {
        let mut params = values.to_vec();
        params.push(int(album.deezer_id));
        db.execute("INSERT INTO albums(title, year, type, genre, owner_id, deezer_id) VALUES (?,?,?,?,?,?)", &params)?;
        Ok(db.last_insert_rowid())
    }
}

/// The song for these artists (Deezer order, main artist first) and title, created if new. Records `track_id` as one of
/// its Deezer track ids.
pub fn upsert_song(
    db: &mut Connection,
    artists: &[(i64, String)],
    title: &str,
    duration: Option<i64>,
    track_id: i64,
) -> Result<i64> {
    let names: Vec<&str> = artists.iter().map(|(_, n)| n.as_str()).collect();
    let key = song_key(&names, title);
    let found = db.query(
        "SELECT id, duration FROM songs WHERE key = ?",
        &[text(&key)],
    )?;
    let id = if let Some(row) = found.rows.first() {
        let id = int_at(row, 0);
        if let (Some(d), None) = (duration, opt_int_at(row, 1)) {
            db.execute(
                "UPDATE songs SET duration = ? WHERE id = ?",
                &[int(d), int(id)],
            )?;
        }
        id
    } else {
        let mut filename = make_filename(&names, title);
        if !db
            .query("SELECT 1 FROM songs WHERE filename = ?", &[text(&filename)])?
            .rows
            .is_empty()
        {
            filename = disambiguate(&filename, &key);
        }
        db.execute(
            "INSERT INTO songs(key, title, filename, duration) VALUES (?,?,?,?)",
            &[
                text(&key),
                text(&clean_title(title)),
                text(&filename),
                duration.filter(|d| *d > 0).map_or(Value::Null, int),
            ],
        )?;
        let id = db.last_insert_rowid();
        for (position, (deezer_id, name)) in artists.iter().enumerate() {
            let artist = upsert_artist(db, *deezer_id, name, None)?;
            db.execute(
                "INSERT OR IGNORE INTO song_artist(song_id, artist_id, position) VALUES (?,?,?)",
                &[int(id), int(artist), int(position as i64)],
            )?;
        }
        id
    };
    db.execute(
        "INSERT OR REPLACE INTO song_track(deezer_id, song_id) VALUES (?, ?)",
        &[int(track_id), int(id)],
    )?;
    Ok(id)
}

pub fn link_album(db: &mut Connection, song: i64, album: i64, track_no: i64) -> Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO song_album(song_id, album_id, track_no) VALUES (?,?,?)",
        &[int(song), int(album), int(track_no)],
    )?;
    Ok(())
}

/// Forgets an album and its place on it (not its songs: `prune` drops those that are on no album).
pub fn drop_album(db: &mut Connection, album: i64) -> Result<u32> {
    let n = db.execute("DELETE FROM song_album WHERE album_id = ?", &[int(album)])?;
    db.execute("DELETE FROM albums WHERE id = ?", &[int(album)])?;
    Ok(n as u32)
}

/// Removes songs that are on no album, with everything that hangs on them, then artists that are in no song, own no album
/// and are not followed. Returns `(songs, artists)` removed.
pub fn prune(db: &mut Connection) -> Result<(u32, u32)> {
    let on_album: HashSet<i64> = db
        .query("SELECT DISTINCT song_id FROM song_album", &[])?
        .rows
        .iter()
        .map(|r| int_at(r, 0))
        .collect();
    let gone: Vec<i64> = db
        .query("SELECT id FROM songs", &[])?
        .rows
        .iter()
        .map(|r| int_at(r, 0))
        .filter(|id| !on_album.contains(id))
        .collect();
    for chunk in gone.chunks(200) {
        let params: Vec<Value> = chunk.iter().map(|id| int(*id)).collect();
        let marks = placeholders(chunk.len());
        for table in ["song_artist", "song_tag", "song_track"] {
            db.execute(
                &format!("DELETE FROM {table} WHERE song_id IN ({marks})"),
                &params,
            )?;
        }
        db.execute(&format!("DELETE FROM songs WHERE id IN ({marks})"), &params)?;
    }
    let used: HashSet<i64> = db
        .query("SELECT DISTINCT artist_id FROM song_artist UNION SELECT DISTINCT owner_id FROM albums WHERE owner_id IS NOT NULL", &[])?
        .rows
        .iter()
        .map(|r| int_at(r, 0))
        .collect();
    let unused: Vec<i64> = db
        .query("SELECT id FROM artists WHERE listed_as IS NULL", &[])?
        .rows
        .iter()
        .map(|r| int_at(r, 0))
        .filter(|id| !used.contains(id))
        .collect();
    for chunk in unused.chunks(200) {
        let params: Vec<Value> = chunk.iter().map(|id| int(*id)).collect();
        db.execute(
            &format!(
                "DELETE FROM artists WHERE id IN ({})",
                placeholders(chunk.len())
            ),
            &params,
        )?;
    }
    drop_unused_tags(db)?;
    Ok((gone.len() as u32, unused.len() as u32))
}

fn drop_unused_tags(db: &mut Connection) -> Result<()> {
    let used: HashSet<i64> = db
        .query("SELECT DISTINCT tag_id FROM song_tag", &[])?
        .rows
        .iter()
        .map(|r| int_at(r, 0))
        .collect();
    for row in db
        .query("SELECT id FROM tags WHERE kind != 'custom'", &[])?
        .rows
    {
        let id = int_at(&row, 0);
        if !used.contains(&id) {
            db.execute("DELETE FROM tags WHERE id = ?", &[int(id)])?;
        }
    }
    Ok(())
}

/// Rebuilds the derived tags. `category_of` maps a followed artist's name to its category (see `follow::category_map`).
/// `category` tags come from the followed artists on a song and the owners of its albums, `genre` tags from its albums'
/// Deezer genres. `custom` tags are never touched. Returns the number of song-tag links.
pub fn retag(db: &mut Connection, category_of: &HashMap<String, String>) -> Result<u32> {
    db.execute("DELETE FROM song_tag WHERE tag_id IN (SELECT id FROM tags WHERE kind IN ('category', 'genre'))", &[])?;
    let mut pairs: BTreeSet<(i64, &str, String)> = BTreeSet::new();
    let category = |name: &str| {
        category_of
            .get(name)
            .cloned()
            .unwrap_or_else(|| UNCATEGORIZED.to_string())
    };
    let on_song = db.query(
        "SELECT sa.song_id, a.listed_as FROM song_artist sa JOIN artists a ON a.id = sa.artist_id WHERE a.listed_as IS NOT NULL",
        &[],
    )?;
    for row in &on_song.rows {
        pairs.insert((int_at(row, 0), "category", category(&str_at(row, 1))));
    }
    let on_album = db.query(
        "SELECT sl.song_id, o.listed_as FROM song_album sl JOIN albums al ON al.id = sl.album_id JOIN artists o ON o.id = al.owner_id WHERE o.listed_as IS NOT NULL",
        &[],
    )?;
    for row in &on_album.rows {
        pairs.insert((int_at(row, 0), "category", category(&str_at(row, 1))));
    }
    let genres = db.query(
        "SELECT sl.song_id, al.genre FROM song_album sl JOIN albums al ON al.id = sl.album_id",
        &[],
    )?;
    for row in &genres.rows {
        let genre = str_at(row, 1);
        for g in if genre.is_empty() {
            "Unknown"
        } else {
            genre.as_str()
        }
        .split("; ")
        {
            pairs.insert((int_at(row, 0), "genre", g.to_string()));
        }
    }
    let mut ids: HashMap<(&str, String), i64> = HashMap::new();
    for (_, kind, name) in &pairs {
        if let std::collections::hash_map::Entry::Vacant(slot) = ids.entry((*kind, name.clone())) {
            db.execute(
                "INSERT OR IGNORE INTO tags(kind, name) VALUES (?, ?)",
                &[text(kind), text(name)],
            )?;
            let row = db.query(
                "SELECT id FROM tags WHERE kind = ? AND name = ?",
                &[text(kind), text(name)],
            )?;
            slot.insert(int_at(&row.rows[0], 0));
        }
    }
    let links: Vec<(i64, i64)> = pairs
        .iter()
        .map(|(song, kind, name)| (*song, ids[&(*kind, name.clone())]))
        .collect();
    for chunk in links.chunks(200) {
        let marks = vec!["(?,?)"; chunk.len()].join(",");
        let params: Vec<Value> = chunk.iter().flat_map(|(s, t)| [int(*s), int(*t)]).collect();
        db.execute(
            &format!("INSERT OR IGNORE INTO song_tag(song_id, tag_id) VALUES {marks}"),
            &params,
        )?;
    }
    drop_unused_tags(db)?;
    Ok(links.len() as u32)
}

/// Artist names of a song, Deezer order (main artist first).
pub fn artists_of(db: &mut Connection, song: i64) -> Result<Vec<String>> {
    let rows = db.query("SELECT a.name FROM song_artist sa JOIN artists a ON a.id = sa.artist_id WHERE sa.song_id = ? ORDER BY sa.position", &[int(song)])?;
    Ok(rows.rows.iter().map(|r| str_at(r, 0)).collect())
}

/// Brings `songs.key` up to the current rule (`KEY_SCHEME`). Keys that would clash with another song's are left as they
/// were, and logged.
pub fn rekey(db: &mut Connection) -> Result<()> {
    if meta_get(db, "key_scheme")?.as_deref() == Some(KEY_SCHEME) {
        return Ok(());
    }
    let mut people: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    for row in db.query("SELECT sa.song_id, sa.position, a.name FROM song_artist sa JOIN artists a ON a.id = sa.artist_id", &[])?.rows {
        people.entry(int_at(&row, 0)).or_default().push((int_at(&row, 1), str_at(&row, 2)));
    }
    let mut changes: Vec<(i64, String)> = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();
    let rows = db.query("SELECT id, key, title FROM songs ORDER BY id", &[])?;
    for row in &rows.rows {
        let id = int_at(row, 0);
        let mut named = people.remove(&id).unwrap_or_default();
        named.sort();
        let names: Vec<&str> = named.iter().map(|(_, n)| n.as_str()).collect();
        let key = song_key(&names, &str_at(row, 2));
        if !taken.insert(key.clone()) {
            quench_log::warn!(
                "rekey: song {id} would get the key of another song ({key}); keeping its old key"
            );
        } else if key != str_at(row, 1) {
            changes.push((id, key));
        }
    }
    db.transaction(|db| {
        let mut pending = changes;
        while !pending.is_empty() {
            let before = pending.len();
            let mut again = Vec::new();
            for (id, key) in pending {
                if db
                    .execute(
                        "UPDATE songs SET key = ? WHERE id = ?",
                        &[text(&key), int(id)],
                    )
                    .is_err()
                {
                    again.push((id, key));
                }
            }
            if again.len() == before {
                for (id, key) in &again {
                    quench_log::warn!(
                        "rekey: song {id} could not take the key {key}; keeping its old key"
                    );
                }
                break;
            }
            pending = again;
        }
        meta_set(db, "key_scheme", KEY_SCHEME)
    })
}
