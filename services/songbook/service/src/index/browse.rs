//! What the library page reads and the edits it makes to a song or an album.

use super::follow::{self, Refused};
use super::songs;
use super::{Result, Value, int, int_at, placeholders, str_at, text};
use quench_sqlite::Connection;
use songbook_core::naming::album_key;
use songbook_core::{Album, Browse, Filter, State, Track};
use std::collections::{HashMap, HashSet};

/// How many tracks the library page shows at most.
pub const LIMIT: usize = 300;

pub fn state(db: &mut Connection) -> Result<State> {
    let genres = db
        .query(
            "SELECT name FROM tags WHERE kind = 'genre' ORDER BY name",
            &[],
        )?
        .rows
        .iter()
        .map(|r| str_at(r, 0))
        .collect();
    Ok(State {
        artists: follow::artist_rows(db)?,
        categories: follow::categories(db)?,
        uncategorized: songs::UNCATEGORIZED.to_string(),
        genres,
        suggestions: follow::suggestions(db)?,
    })
}

/// Artist names per song id, main artist first, for the given songs (all songs when `only` is `None`).
fn people(db: &mut Connection, only: Option<&[i64]>) -> Result<HashMap<i64, Vec<String>>> {
    let mut out: HashMap<i64, Vec<String>> = HashMap::new();
    let base =
        "SELECT sa.song_id, a.name FROM song_artist sa JOIN artists a ON a.id = sa.artist_id";
    let rows = match only {
        None => {
            db.query(&format!("{base} ORDER BY sa.song_id, sa.position"), &[])?
                .rows
        }
        Some(ids) => {
            let mut all = Vec::new();
            for chunk in ids.chunks(200) {
                let params: Vec<Value> = chunk.iter().map(|i| int(*i)).collect();
                all.extend(
                    db.query(
                        &format!(
                            "{base} WHERE sa.song_id IN ({}) ORDER BY sa.song_id, sa.position",
                            placeholders(chunk.len())
                        ),
                        &params,
                    )?
                    .rows,
                );
            }
            all
        }
    };
    for row in rows {
        out.entry(int_at(&row, 0))
            .or_default()
            .push(str_at(&row, 1));
    }
    Ok(out)
}

/// One song on one album, with everything a filter looks at, as the library page reads it.
#[derive(Debug, Clone)]
struct Row {
    song: i64,
    title: String,
    url: String,
    official: bool,
    manual: bool,
    album_id: i64,
    album: String,
    year: String,
    kind: String,
    genre: String,
    owner: String,
    people: String,
    /// Lowercase title, album and artists: what the search box looks in.
    haystack: String,
    categories: HashSet<String>,
    genres: HashSet<String>,
}

/// The library as the page reads it, in the order it shows it. Built from the tables when asked for and kept until the next
/// write (`Index::snapshot`), so browsing does not scan the database each time.
#[derive(Debug, Clone)]
pub struct Snapshot {
    rows: Vec<Row>,
    pub state: State,
}

impl Snapshot {
    /// Reads the whole library: the joined, ordered tracklists, who is on each song and its tags.
    pub fn load(db: &mut Connection) -> Result<Self> {
        let rows = db
            .query(
                "SELECT s.id, s.title, s.yt_url, s.yt_official, s.yt_manual, al.deezer_id, al.title, al.year, al.type, al.genre, o.listed_as \
                 FROM song_album sl JOIN songs s ON s.id = sl.song_id JOIN albums al ON al.id = sl.album_id JOIN artists o ON o.id = al.owner_id \
                 ORDER BY o.listed_as, al.year, al.title, sl.track_no",
                &[],
            )?
            .rows;
        let who = people(db, None)?;
        let mut tags: HashMap<i64, (HashSet<String>, HashSet<String>)> = HashMap::new();
        for row in db
            .query(
                "SELECT st.song_id, t.kind, t.name FROM song_tag st JOIN tags t ON t.id = st.tag_id",
                &[],
            )?
            .rows
        {
            let entry = tags.entry(int_at(&row, 0)).or_default();
            match str_at(&row, 1).as_str() {
                "category" => entry.0.insert(str_at(&row, 2)),
                "genre" => entry.1.insert(str_at(&row, 2)),
                _ => false,
            };
        }
        let rows = rows
            .iter()
            .map(|r| {
                let song = int_at(r, 0);
                let people = who.get(&song).map(|n| n.join(", ")).unwrap_or_default();
                let (categories, genres) = tags.get(&song).cloned().unwrap_or_default();
                Row {
                    song,
                    title: str_at(r, 1),
                    url: str_at(r, 2),
                    official: int_at(r, 3) != 0,
                    manual: int_at(r, 4) != 0,
                    album_id: int_at(r, 5),
                    haystack: format!("{} {} {}", str_at(r, 1), str_at(r, 6), people)
                        .to_lowercase(),
                    album: str_at(r, 6),
                    year: str_at(r, 7),
                    kind: str_at(r, 8),
                    genre: str_at(r, 9),
                    owner: str_at(r, 10),
                    people,
                    categories,
                    genres,
                }
            })
            .collect();
        Ok(Self {
            rows,
            state: state(db)?,
        })
    }

    /// Albums of the followed artists with their tracklists. A song on several albums is listed on each; `total` and
    /// `linked` count every matching track, `albums` holds the first `LIMIT`.
    #[must_use]
    pub fn browse(&self, filter: &Filter) -> Browse {
        let needle = filter.q.to_lowercase();
        let matching: Vec<&Row> = self
            .rows
            .iter()
            .filter(|r| {
                (filter.artist.is_empty() || r.owner == filter.artist)
                    && (filter.category.is_empty() || r.categories.contains(&filter.category))
                    && (filter.genre.is_empty() || r.genres.contains(&filter.genre))
                    && (needle.is_empty() || r.haystack.contains(&needle))
                    && match filter.links.as_str() {
                        "missing" => r.url.is_empty(),
                        "has" => !r.url.is_empty(),
                        "unofficial" => !r.url.is_empty() && !r.official,
                        _ => true,
                    }
            })
            .collect();
        let total = matching.len();
        let linked = matching.iter().filter(|r| !r.url.is_empty()).count();
        let mut albums: Vec<Album> = Vec::new();
        let mut at: HashMap<(&str, i64), usize> = HashMap::new();
        for row in matching.into_iter().take(LIMIT) {
            let index = *at
                .entry((row.owner.as_str(), row.album_id))
                .or_insert_with(|| {
                    albums.push(Album {
                        artist: row.owner.clone(),
                        album: row.album.clone(),
                        year: row.year.clone(),
                        id: row.album_id,
                        kind: row.kind.clone(),
                        genre: row.genre.clone(),
                        tracks: Vec::new(),
                    });
                    albums.len() - 1
                });
            albums[index].tracks.push(Track {
                id: row.song,
                title: row.title.clone(),
                artists: row.people.clone(),
                url: row.url.clone(),
                official: row.official,
                manual: row.manual,
            });
        }
        let shown = albums.iter().map(|a| a.tracks.len()).sum::<usize>();
        Browse {
            total: total as u32,
            linked: linked as u32,
            shown: shown as u32,
            albums,
        }
    }
}

/// Albums of the followed artists with their tracklists, read straight from the tables (the service uses a [`Snapshot`]).
pub fn browse(db: &mut Connection, filter: &Filter) -> Result<Browse> {
    Ok(Snapshot::load(db)?.browse(filter))
}

/// Is this a link a browser can open (`http(s)://host...`)?
#[must_use]
pub fn is_link(url: &str) -> bool {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .is_some_and(|rest| !rest.split('/').next().unwrap_or_default().is_empty())
}

/// Sets the YouTube link of a song, or clears it with an empty `url` (the next lookup will try again).
pub fn set_link(
    db: &mut Connection,
    song: i64,
    url: &str,
) -> Result<std::result::Result<u32, Refused>> {
    let url = url.trim();
    if !url.is_empty() && !is_link(url) {
        return Ok(Err(Refused::Input(
            "URL must start with http:// or https://".into(),
        )));
    }
    if db
        .query("SELECT 1 FROM songs WHERE id = ?", &[int(song)])?
        .rows
        .is_empty()
    {
        return Ok(Err(Refused::Input("unknown track".into())));
    }
    if url.is_empty() {
        db.execute(
            "UPDATE songs SET yt_url = '', yt_channel = '', yt_title = '', yt_official = 0, yt_score = NULL, yt_manual = 0, yt_status = NULL, yt_candidate = '' WHERE id = ?",
            &[int(song)],
        )?;
    } else {
        db.execute(
            "UPDATE songs SET yt_url = ?, yt_channel = 'manual', yt_title = '', yt_official = 0, yt_score = NULL, yt_manual = 1, yt_status = 'ok', yt_candidate = '' WHERE id = ?",
            &[text(url), int(song)],
        )?;
    }
    Ok(Ok(1))
}

/// Blacklists the album (and its other editions) of an artist and drops it, and the songs only it held, from the index.
/// Returns how many tracks left the albums.
pub fn blacklist_album(
    db: &mut Connection,
    artist: &str,
    album_id: i64,
) -> Result<std::result::Result<u32, Refused>> {
    db.transaction(|db| {
        let found = db.query(
            "SELECT al.title FROM albums al JOIN artists o ON o.id = al.owner_id WHERE o.listed_as = ? AND al.deezer_id = ?",
            &[text(artist), int(album_id)],
        )?;
        let Some(row) = found.rows.first() else {
            return Ok(Err(Refused::Input("unknown album".into())));
        };
        let title = str_at(row, 0);
        let key = album_key(&title);
        follow::blacklist_add(db, artist, &title)?;
        let owned = db.query("SELECT al.id, al.title FROM albums al JOIN artists o ON o.id = al.owner_id WHERE o.listed_as = ?", &[text(artist)])?;
        let mut removed = 0;
        for al in owned.rows.iter().filter(|r| album_key(&str_at(r, 1)) == key) {
            removed += songs::drop_album(db, int_at(al, 0))?;
        }
        songs::prune(db)?;
        let cmap = follow::category_map(db)?;
        songs::retag(db, &cmap)?;
        Ok(Ok(removed))
    })
}

/// Songs by a followed artist (the name as followed or as Deezer spells it), as song ids.
pub fn songs_of(db: &mut Connection, artist: &str) -> Result<HashSet<i64>> {
    let rows = db.query(
        "SELECT sa.song_id FROM song_artist sa JOIN artists a ON a.id = sa.artist_id WHERE a.name = ? OR a.listed_as = ?",
        &[text(artist), text(artist)],
    )?;
    Ok(rows.rows.iter().map(|r| int_at(r, 0)).collect())
}
