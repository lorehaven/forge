//! What the user manages: followed artists, categories, the album blacklist, and the last suggestions.

use super::songs::UNCATEGORIZED;
use super::{Db, Result, Value, int, int_at, opt_str_at, str_at, text};
use songbook_core::naming::{album_key, norm};
use songbook_core::{ArtistRow, Suggestion, TopTrack};
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
    /// Only albums in these genres are kept (`; `-separated); empty keeps all.
    pub genres: String,
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
        genres: str_at(row, 7),
    }
}

const FOLLOW_COLUMNS: &str = "name, pin, category, deezer_name, deezer_id, fans, note, genres";

pub async fn follows(db: &mut Db) -> Result<Vec<Follow>> {
    let rows = db
        .query(
            &format!("SELECT {FOLLOW_COLUMNS} FROM follows ORDER BY position, id"),
            &[],
        )
        .await?;
    Ok(rows.rows.iter().map(|r| follow_from(r)).collect())
}

pub async fn categories(db: &mut Db) -> Result<Vec<String>> {
    let rows = db
        .query("SELECT name FROM categories ORDER BY position, id", &[])
        .await?;
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
pub async fn known_category(
    db: &mut Db,
    category: &str,
) -> Result<std::result::Result<String, Refused>> {
    let category = clean(category);
    if category.is_empty() || norm(&category) == norm(UNCATEGORIZED) {
        return Ok(Ok(String::new()));
    }
    let known = categories(db).await?;
    Ok(known
        .into_iter()
        .find(|c| norm(c) == norm(&category))
        .ok_or_else(|| Refused::Input(format!("unknown category: {category}"))))
}

/// Followed artist name to its category; artists with none (or an unknown one) map to `Uncategorized`.
pub async fn category_map(db: &mut Db) -> Result<HashMap<String, String>> {
    let known: HashMap<String, String> = categories(db)
        .await?
        .into_iter()
        .map(|c| (norm(&c), c))
        .collect();
    Ok(follows(db)
        .await?
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
pub async fn add_follow(
    db: &mut Db,
    name: &str,
    pin: &str,
    category: &str,
) -> Result<std::result::Result<bool, Refused>> {
    let name = clean(name);
    if name.is_empty() {
        return Ok(Err(Refused::Input("empty name".into())));
    }
    let category = match known_category(db, category).await? {
        Ok(c) => c,
        Err(e) => return Ok(Err(e)),
    };
    if follows(db)
        .await?
        .iter()
        .any(|f| norm(&f.name) == norm(&name))
    {
        return Ok(Ok(false));
    }
    let pin: String = pin.chars().filter(char::is_ascii_digit).collect();
    let position = db
        .query("SELECT COALESCE(MAX(position), 0) + 1 FROM follows", &[])
        .await?
        .rows
        .first()
        .map_or(1, |r| int_at(r, 0));
    db.execute(
        "INSERT INTO follows(name, pin, category, position) VALUES (?,?,?,?)",
        &[text(&name), text(&pin), text(&category), int(position)],
    )
    .await?;
    Ok(Ok(true))
}

/// Stops following. The artist's albums leave the index at the next sync.
pub async fn remove_follow(db: &mut Db, name: &str) -> Result<()> {
    for f in follows(db).await? {
        if norm(&f.name) == norm(name) {
            db.execute("DELETE FROM follows WHERE name = ?", &[text(&f.name)])
                .await?;
        }
    }
    Ok(())
}

/// Sets a followed artist's category (empty for none). `Err` for an unknown artist or category.
pub async fn set_category(
    db: &mut Db,
    name: &str,
    category: &str,
) -> Result<std::result::Result<(), Refused>> {
    let category = match known_category(db, category).await? {
        Ok(c) => c,
        Err(e) => return Ok(Err(e)),
    };
    let Some(f) = follows(db)
        .await?
        .into_iter()
        .find(|f| norm(&f.name) == norm(name))
    else {
        return Ok(Err(Refused::Input("unknown artist".into())));
    };
    db.execute(
        "UPDATE follows SET category = ? WHERE name = ?",
        &[text(&category), text(&f.name)],
    )
    .await?;
    Ok(Ok(()))
}

/// The genres a name is split into: `; `-separated, trimmed, empty ones dropped.
#[must_use]
pub fn genre_list(genres: &str) -> Vec<String> {
    genres
        .split([';', '|'])
        .map(clean)
        .filter(|g| !g.is_empty())
        .collect()
}

/// Sets the genres an artist's albums must have to be kept (empty: any). The next sync applies it.
pub async fn set_genres(
    db: &mut Db,
    name: &str,
    genres: &str,
) -> Result<std::result::Result<(), Refused>> {
    let Some(f) = follows(db)
        .await?
        .into_iter()
        .find(|f| norm(&f.name) == norm(name))
    else {
        return Ok(Err(Refused::Input("unknown artist".into())));
    };
    db.execute(
        "UPDATE follows SET genres = ? WHERE name = ?",
        &[text(&genre_list(genres).join("; ")), text(&f.name)],
    )
    .await?;
    Ok(Ok(()))
}

/// Adds a category. `Ok(false)` when it exists.
pub async fn add_category(db: &mut Db, name: &str) -> Result<std::result::Result<bool, Refused>> {
    let name = clean(name);
    if name.is_empty() || norm(&name) == norm(UNCATEGORIZED) {
        return Ok(Err(Refused::Input("invalid category name".into())));
    }
    let known = categories(db).await?;
    if known.iter().any(|c| norm(c) == norm(&name)) {
        return Ok(Ok(false));
    }
    db.execute(
        "INSERT INTO categories(name, position) VALUES (?, ?)",
        &[text(&name), int(known.len() as i64 + 1)],
    )
    .await?;
    Ok(Ok(true))
}

/// Renames a category; the artists in it keep their place. Only a case change may land on a name that is taken (by itself).
pub async fn rename_category(
    db: &mut Db,
    name: &str,
    new_name: &str,
) -> Result<std::result::Result<(), Refused>> {
    let old = match known_category(db, name).await? {
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
    if norm(&new) != norm(&old) && categories(db).await?.iter().any(|c| norm(c) == norm(&new)) {
        return Ok(Err(Refused::Input(format!("category exists: {new}"))));
    }
    db.execute(
        "UPDATE categories SET name = ? WHERE name = ?",
        &[text(&new), text(&old)],
    )
    .await?;
    db.execute(
        "UPDATE follows SET category = ? WHERE category = ?",
        &[text(&new), text(&old)],
    )
    .await?;
    Ok(Ok(()))
}

/// Blacklisted `(normalised artist, album key)` pairs.
pub async fn blacklist(db: &mut Db) -> Result<HashSet<(String, String)>> {
    let rows = db
        .query("SELECT artist, album_key FROM blacklist", &[])
        .await?;
    Ok(rows
        .rows
        .iter()
        .map(|r| (norm(&str_at(r, 0)), str_at(r, 1)))
        .collect())
}

/// Blacklists every edition of an album of this artist.
pub async fn blacklist_add(db: &mut Db, artist: &str, title: &str) -> Result<()> {
    let key = album_key(title);
    if !blacklist(db).await?.contains(&(norm(artist), key.clone())) {
        db.execute(
            "INSERT INTO blacklist(artist, album_key, title) VALUES (?,?,?) ON CONFLICT DO NOTHING",
            &[text(&clean(artist)), text(&key), text(&clean(title))],
        )
        .await?;
    }
    Ok(())
}

/// Every blacklisted album, by artist and title.
pub async fn blacklist_entries(db: &mut Db) -> Result<Vec<(i64, String, String)>> {
    let rows = db
        .query(
            "SELECT id, artist, title FROM blacklist ORDER BY lower(artist) COLLATE \"C\", lower(title) COLLATE \"C\", id",
            &[],
        )
        .await?;
    Ok(rows
        .rows
        .iter()
        .map(|r| (int_at(r, 0), str_at(r, 1), str_at(r, 2)))
        .collect())
}

/// Lets an album back in: the entry goes. `false` when there is no such entry.
pub async fn blacklist_remove(db: &mut Db, id: i64) -> Result<bool> {
    let had = !db
        .query("SELECT 1 FROM blacklist WHERE id = ?", &[int(id)])
        .await?
        .rows
        .is_empty();
    db.execute("DELETE FROM blacklist WHERE id = ?", &[int(id)])
        .await?;
    Ok(had)
}

/// The sidebar rows: followed artists with their match and song count.
pub async fn artist_rows(db: &mut Db) -> Result<Vec<ArtistRow>> {
    let counts: HashMap<String, i64> = db
        .query(
            // The songs of the artist's own discography (the albums found under them), each once: the number the
            // library page shows when the artist is selected.
            "SELECT o.listed_as, COUNT(DISTINCT sl.song_id) FROM albums al JOIN artists o ON o.id = al.owner_id \
             JOIN song_album sl ON sl.album_id = al.id WHERE o.listed_as IS NOT NULL GROUP BY o.id",
            &[],
        ).await?
        .rows
        .iter()
        .map(|r| (str_at(r, 0), int_at(r, 1)))
        .collect();
    let cmap = category_map(db).await?;
    Ok(follows(db)
        .await?
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
            genres: f.genres,
            name: f.name,
        })
        .collect())
}

pub async fn suggestions(db: &mut Db) -> Result<Vec<Suggestion>> {
    let rows = db.query("SELECT artist, deezer_id, fans, related_to_count, related_to, scope FROM suggestions ORDER BY position, id", &[]).await?;
    let mut best: HashMap<String, Vec<TopTrack>> = HashMap::new();
    for r in db
        .query("SELECT deezer_id, title, album, score FROM suggestion_tracks ORDER BY deezer_id COLLATE \"C\", position", &[]).await?
        .rows
    {
        best.entry(str_at(&r, 0)).or_default().push(TopTrack {
            title: str_at(&r, 1),
            album: str_at(&r, 2),
            score: int_at(&r, 3) as u32,
        });
    }
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
            best: best.remove(&str_at(r, 1)).unwrap_or_default(),
        })
        .collect())
}

/// Replaces the suggestions. Run it inside a transaction when the caller wants all or nothing.
pub async fn set_suggestions(db: &mut Db, list: &[Suggestion]) -> Result<()> {
    db.execute("DELETE FROM suggestions", &[]).await?;
    db.execute("DELETE FROM suggestion_tracks", &[]).await?;
    for (i, s) in list.iter().enumerate() {
        db.execute(
            "INSERT INTO suggestions(artist, deezer_id, fans, related_to_count, related_to, scope, position) VALUES (?,?,?,?,?,?,?)",
            &[text(&s.artist), text(&s.deezer_id), text(&s.fans), int(i64::from(s.related_to_count)), text(&s.related_to), text(&s.scope), int(i as i64)],
        ).await?;
        for (n, t) in s.best.iter().enumerate() {
            db.execute(
                "INSERT INTO suggestion_tracks(deezer_id, position, title, album, score) VALUES (?,?,?,?,?) ON CONFLICT (deezer_id, position) DO UPDATE SET title = EXCLUDED.title, album = EXCLUDED.album, score = EXCLUDED.score",
                &[text(&s.deezer_id), int(n as i64), text(&t.title), text(&t.album), int(i64::from(t.score))],
            ).await?;
        }
    }
    Ok(())
}

/// Records what a sync found for a followed artist (`note` empty when matched, `NO MATCH` otherwise).
pub async fn set_match(
    db: &mut Db,
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
    )
    .await?;
    Ok(())
}
