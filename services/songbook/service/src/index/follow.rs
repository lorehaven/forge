//! What the user manages: followed artists, categories, the album blacklist, and the last suggestions.

use super::songs::UNCATEGORIZED;
use super::{Result, Value, int, int_at, opt_str_at, str_at, text};
use quench_sqlite::Connection;
use songbook_core::naming::{album_key, norm};
use songbook_core::{ArtistRow, Suggestion};
use std::collections::{HashMap, HashSet};

/// A followed artist and what the last sync found for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Follow {
    pub name: String,
    /// A Deezer artist id given by hand; empty to match by name.
    pub pin: String,
    pub category: String,
    pub deezer_name: String,
    pub deezer_id: String,
    pub fans: String,
    /// `None` before the first sync; empty when matched; `NO MATCH` when Deezer had nobody by that name.
    pub note: Option<String>,
}

/// Why a change was refused (shown to the user) or could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    Input(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(message) => f.write_str(message),
        }
    }
}

fn follow_from(row: &[Value]) -> Follow {
    Follow {
        name: str_at(row, 0),
        pin: str_at(row, 1),
        category: str_at(row, 2),
        deezer_name: str_at(row, 3),
        deezer_id: str_at(row, 4),
        fans: str_at(row, 5),
        note: opt_str_at(row, 6),
    }
}

const FOLLOW_COLUMNS: &str = "name, pin, category, deezer_name, deezer_id, fans, note";

pub fn follows(db: &mut Connection) -> Result<Vec<Follow>> {
    let rows = db.query(
        &format!("SELECT {FOLLOW_COLUMNS} FROM follows ORDER BY position, id"),
        &[],
    )?;
    Ok(rows.rows.iter().map(|r| follow_from(r)).collect())
}

pub fn categories(db: &mut Connection) -> Result<Vec<String>> {
    let rows = db.query("SELECT name FROM categories ORDER BY position, id", &[])?;
    Ok(rows.rows.iter().map(|r| str_at(r, 0)).collect())
}

/// Names a free-form value can be written under: no line breaks, `|` or `#` (the old text files' separators), trimmed.
#[must_use]
pub fn clean(value: &str) -> String {
    let replaced: String = value
        .chars()
        .map(|c| {
            if matches!(c, '\r' | '\n' | '|' | '#') {
                ' '
            } else {
                c
            }
        })
        .collect();
    replaced.trim().to_string()
}

/// A category that exists, or empty for none. `Err` names an unknown one.
pub fn known_category(
    db: &mut Connection,
    category: &str,
) -> Result<std::result::Result<String, Refused>> {
    let category = clean(category);
    if category.is_empty() || norm(&category) == norm(UNCATEGORIZED) {
        return Ok(Ok(String::new()));
    }
    let known = categories(db)?;
    Ok(known
        .into_iter()
        .find(|c| norm(c) == norm(&category))
        .ok_or_else(|| Refused::Input(format!("unknown category: {category}"))))
}

/// Followed artist name to its category; artists with none (or an unknown one) map to `Uncategorized`.
pub fn category_map(db: &mut Connection) -> Result<HashMap<String, String>> {
    let known: HashMap<String, String> =
        categories(db)?.into_iter().map(|c| (norm(&c), c)).collect();
    Ok(follows(db)?
        .into_iter()
        .map(|f| {
            let category = known
                .get(&norm(&f.category))
                .cloned()
                .unwrap_or_else(|| UNCATEGORIZED.to_string());
            (f.name, category)
        })
        .collect())
}

/// Follows a new artist. `Ok(false)` when an artist of that name is already followed.
pub fn add_follow(
    db: &mut Connection,
    name: &str,
    pin: &str,
    category: &str,
) -> Result<std::result::Result<bool, Refused>> {
    let name = clean(name);
    if name.is_empty() {
        return Ok(Err(Refused::Input("empty name".into())));
    }
    let category = match known_category(db, category)? {
        Ok(c) => c,
        Err(e) => return Ok(Err(e)),
    };
    if follows(db)?.iter().any(|f| norm(&f.name) == norm(&name)) {
        return Ok(Ok(false));
    }
    let pin: String = pin.chars().filter(char::is_ascii_digit).collect();
    let position = db
        .query("SELECT COALESCE(MAX(position), 0) + 1 FROM follows", &[])?
        .rows
        .first()
        .map_or(1, |r| int_at(r, 0));
    db.execute(
        "INSERT INTO follows(name, pin, category, position) VALUES (?,?,?,?)",
        &[text(&name), text(&pin), text(&category), int(position)],
    )?;
    Ok(Ok(true))
}

/// Stops following. The artist's albums leave the index at the next sync.
pub fn remove_follow(db: &mut Connection, name: &str) -> Result<()> {
    for f in follows(db)? {
        if norm(&f.name) == norm(name) {
            db.execute("DELETE FROM follows WHERE name = ?", &[text(&f.name)])?;
        }
    }
    Ok(())
}

/// Sets a followed artist's category (empty for none). `Err` for an unknown artist or category.
pub fn set_category(
    db: &mut Connection,
    name: &str,
    category: &str,
) -> Result<std::result::Result<(), Refused>> {
    let category = match known_category(db, category)? {
        Ok(c) => c,
        Err(e) => return Ok(Err(e)),
    };
    let Some(f) = follows(db)?
        .into_iter()
        .find(|f| norm(&f.name) == norm(name))
    else {
        return Ok(Err(Refused::Input("unknown artist".into())));
    };
    db.execute(
        "UPDATE follows SET category = ? WHERE name = ?",
        &[text(&category), text(&f.name)],
    )?;
    Ok(Ok(()))
}

/// Adds a category. `Ok(false)` when it exists.
pub fn add_category(db: &mut Connection, name: &str) -> Result<std::result::Result<bool, Refused>> {
    let name = clean(name);
    if name.is_empty() || norm(&name) == norm(UNCATEGORIZED) {
        return Ok(Err(Refused::Input("invalid category name".into())));
    }
    let known = categories(db)?;
    if known.iter().any(|c| norm(c) == norm(&name)) {
        return Ok(Ok(false));
    }
    db.execute(
        "INSERT INTO categories(name, position) VALUES (?, ?)",
        &[text(&name), int(known.len() as i64 + 1)],
    )?;
    Ok(Ok(true))
}

/// Renames a category; the artists in it keep their place. Only a case change may land on a name that is taken (by itself).
pub fn rename_category(
    db: &mut Connection,
    name: &str,
    new_name: &str,
) -> Result<std::result::Result<(), Refused>> {
    let old = match known_category(db, name)? {
        Ok(c) if !c.is_empty() => c,
        Ok(_) => {
            return Ok(Err(Refused::Input(
                "Uncategorized cannot be renamed".into(),
            )));
        }
        Err(e) => return Ok(Err(e)),
    };
    let new = clean(new_name);
    if new.is_empty() || norm(&new) == norm(UNCATEGORIZED) {
        return Ok(Err(Refused::Input("invalid category name".into())));
    }
    if norm(&new) != norm(&old) && categories(db)?.iter().any(|c| norm(c) == norm(&new)) {
        return Ok(Err(Refused::Input(format!("category exists: {new}"))));
    }
    db.execute(
        "UPDATE categories SET name = ? WHERE name = ?",
        &[text(&new), text(&old)],
    )?;
    db.execute(
        "UPDATE follows SET category = ? WHERE category = ?",
        &[text(&new), text(&old)],
    )?;
    Ok(Ok(()))
}

/// Blacklisted `(normalised artist, album key)` pairs.
pub fn blacklist(db: &mut Connection) -> Result<HashSet<(String, String)>> {
    let rows = db.query("SELECT artist, album_key FROM blacklist", &[])?;
    Ok(rows
        .rows
        .iter()
        .map(|r| (norm(&str_at(r, 0)), str_at(r, 1)))
        .collect())
}

/// Blacklists every edition of an album of this artist.
pub fn blacklist_add(db: &mut Connection, artist: &str, title: &str) -> Result<()> {
    let key = album_key(title);
    if !blacklist(db)?.contains(&(norm(artist), key.clone())) {
        db.execute(
            "INSERT OR IGNORE INTO blacklist(artist, album_key, title) VALUES (?,?,?)",
            &[text(&clean(artist)), text(&key), text(&clean(title))],
        )?;
    }
    Ok(())
}

/// The sidebar rows: followed artists with their match and song count.
pub fn artist_rows(db: &mut Connection) -> Result<Vec<ArtistRow>> {
    let counts: HashMap<String, i64> = db
        .query(
            // The songs of the artist's own discography (the albums found under them), each once: the number the
            // library page shows when the artist is selected.
            "SELECT o.listed_as, COUNT(DISTINCT sl.song_id) FROM albums al JOIN artists o ON o.id = al.owner_id \
             JOIN song_album sl ON sl.album_id = al.id WHERE o.listed_as IS NOT NULL GROUP BY o.id",
            &[],
        )?
        .rows
        .iter()
        .map(|r| (str_at(r, 0), int_at(r, 1)))
        .collect();
    let cmap = category_map(db)?;
    Ok(follows(db)?
        .into_iter()
        .map(|f| ArtistRow {
            pinned: !f.pin.is_empty(),
            category: cmap
                .get(&f.name)
                .cloned()
                .unwrap_or_else(|| UNCATEGORIZED.to_string()),
            tracks: counts.get(&f.name).copied().unwrap_or(0) as u32,
            synced: f.note.is_some(),
            deezer_name: f.deezer_name,
            deezer_id: f.deezer_id,
            fans: f.fans,
            name: f.name,
        })
        .collect())
}

pub fn suggestions(db: &mut Connection) -> Result<Vec<Suggestion>> {
    let rows = db.query("SELECT artist, deezer_id, fans, related_to_count, related_to, scope FROM suggestions ORDER BY position, id", &[])?;
    Ok(rows
        .rows
        .iter()
        .map(|r| Suggestion {
            artist: str_at(r, 0),
            deezer_id: str_at(r, 1),
            fans: str_at(r, 2),
            related_to_count: int_at(r, 3) as u32,
            related_to: str_at(r, 4),
            scope: str_at(r, 5),
        })
        .collect())
}

/// Replaces the suggestions. Run it inside a transaction when the caller wants all or nothing.
pub fn set_suggestions(db: &mut Connection, list: &[Suggestion]) -> Result<()> {
    db.execute("DELETE FROM suggestions", &[])?;
    for (i, s) in list.iter().enumerate() {
        db.execute(
            "INSERT INTO suggestions(artist, deezer_id, fans, related_to_count, related_to, scope, position) VALUES (?,?,?,?,?,?,?)",
            &[text(&s.artist), text(&s.deezer_id), text(&s.fans), int(i64::from(s.related_to_count)), text(&s.related_to), text(&s.scope), int(i as i64)],
        )?;
    }
    Ok(())
}

/// Records what a sync found for a followed artist (`note` empty when matched, `NO MATCH` otherwise).
pub fn set_match(
    db: &mut Connection,
    name: &str,
    deezer_name: &str,
    deezer_id: &str,
    fans: &str,
    note: &str,
) -> Result<()> {
    db.execute(
        "UPDATE follows SET deezer_name = ?, deezer_id = ?, fans = ?, note = ? WHERE name = ?",
        &[
            text(deezer_name),
            text(deezer_id),
            text(fans),
            text(note),
            text(name),
        ],
    )?;
    Ok(())
}
