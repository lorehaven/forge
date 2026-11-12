//! What the library page reads and the edits it makes to a song or an album.

use super::follow::{self, Refused};
use super::songs;
use super::{Db, Result, Value, int, int_at, opt_int_at, placeholders, str_at, text};
use songbook_core::naming::album_key;
use songbook_core::{Album, Browse, Filter, State, Stats, Track, popularity};
use std::collections::{HashMap, HashSet};

/// How many tracks the library page shows at most.
pub const LIMIT: usize = 300;

/// How many songs of an artist the "top artists" order lists.
pub const BEST_PER_ARTIST: usize = 10;

pub async fn state(db: &mut Db) -> Result<State> {
    let genres = db
        .query(
            "SELECT name FROM tags WHERE kind = 'genre' ORDER BY name COLLATE \"C\"",
            &[],
        )
        .await?
        .rows
        .iter()
        .map(|r| str_at(r, 0))
        .collect();
    Ok(State {
        artists: follow::artist_rows(db).await?,
        categories: follow::categories(db).await?,
        uncategorized: songs::UNCATEGORIZED.to_string(),
        genres,
        suggestions: follow::suggestions(db).await?,
    })
}

/// Artist names per song id, main artist first, for the given songs (all songs when `only` is `None`).
async fn people(db: &mut Db, only: Option<&[i64]>) -> Result<HashMap<i64, Vec<String>>> {
    let mut out: HashMap<i64, Vec<String>> = HashMap::new();
    let base =
        "SELECT sa.song_id, a.name FROM song_artist sa JOIN artists a ON a.id = sa.artist_id";
    let rows = match only {
        None => {
            db.query(&format!("{base} ORDER BY sa.song_id, sa.position"), &[])
                .await?
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
                    )
                    .await?
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
    /// File name without extension, to tell whether the song is on disk.
    filename: String,
    score: Option<u32>,
    /// Deezer's rank, to tell apart songs whose rounded scores are equal.
    rank: i64,
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
    /// `(file name without extension, has a YouTube link)` for every song, for the statistics.
    files: Vec<(String, bool)>,
    followed: u32,
    credited: u32,
    albums: u32,
}

impl Snapshot {
    /// Reads the whole library: the joined, ordered tracklists, who is on each song and its tags.
    pub async fn load(db: &mut Db) -> Result<Self> {
        let rows = db
            .query(
                "SELECT s.id, s.title, s.yt_url, s.yt_official, s.yt_manual, al.deezer_id, al.title, al.year, al.type, al.genre, o.listed_as, s.filename, s.rank, s.yt_views \
                 FROM song_album sl JOIN songs s ON s.id = sl.song_id JOIN albums al ON al.id = sl.album_id JOIN artists o ON o.id = al.owner_id \
                 ORDER BY o.listed_as COLLATE \"C\", al.year, al.title COLLATE \"C\", sl.track_no",
                &[],
            ).await?
            .rows;
        let who = people(db, None).await?;
        let mut tags: HashMap<i64, (HashSet<String>, HashSet<String>)> = HashMap::new();
        for row in db
            .query(
                "SELECT st.song_id, t.kind, t.name FROM song_tag st JOIN tags t ON t.id = st.tag_id",
                &[],
            ).await?
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
                    filename: str_at(r, 11),
                    score: popularity(opt_int_at(r, 12), opt_int_at(r, 13)),
                    rank: opt_int_at(r, 12).unwrap_or(0),
                    people,
                    categories,
                    genres,
                }
            })
            .collect();
        let count = async |db: &mut Db, table: &str| -> Result<u32> {
            Ok(db
                .query(&format!("SELECT COUNT(*) FROM {table}"), &[])
                .await?
                .rows
                .first()
                .map_or(0, |r| int_at(r, 0)) as u32)
        };
        let files = db
            .query("SELECT filename, yt_url FROM songs", &[])
            .await?
            .rows
            .iter()
            .map(|r| (str_at(r, 0), !str_at(r, 1).is_empty()))
            .collect();
        Ok(Self {
            rows,
            state: state(db).await?,
            files,
            followed: count(db, "follows").await?,
            credited: count(db, "artists").await?,
            albums: count(db, "albums").await?,
        })
    }

    /// The size of the library and how much of it is on disk, given the files in the audio folder (`files::file_index`).
    #[must_use]
    pub fn stats(&self, on_disk: &HashMap<String, Vec<String>>) -> Stats {
        let downloaded = self
            .files
            .iter()
            .filter(|(name, _)| on_disk.contains_key(name))
            .count();
        let missing = self.files.len() - downloaded;
        let missing_linked = self
            .files
            .iter()
            .filter(|(name, linked)| *linked && !on_disk.contains_key(name))
            .count();
        Stats {
            artists: self.followed,
            credited: self.credited,
            albums: self.albums,
            songs: self.files.len() as u32,
            downloaded: downloaded as u32,
            missing: missing as u32,
            missing_linked: missing_linked as u32,
        }
    }

    /// Albums of the followed artists with their tracklists. A song on several albums is listed on each; `total` and
    /// `linked` count each matching song once, `albums` holds the first `LIMIT` listed tracks.
    #[must_use]
    pub fn browse(&self, filter: &Filter) -> Browse {
        self.browse_on_disk(filter, &HashMap::new())
    }

    /// [`browse`](Self::browse) that also knows which files are on disk (`files::file_index`), which `sort=suggested` needs.
    ///
    /// `sort=artists` lists the artists with most Deezer fans first, each with their best songs.
    /// `sort=top` lists the matching songs, each once, most popular first; `sort=suggested` does the same for the ones
    /// that are not downloaded yet. Songs nobody scored come last (top) or not at all (suggested). The list is one
    /// "album" with no name, so the page shows it as a plain list.
    #[must_use]
    pub fn browse_on_disk(
        &self,
        filter: &Filter,
        on_disk: &HashMap<String, Vec<String>>,
    ) -> Browse {
        let needle = filter.q.to_lowercase();
        let mut matching: Vec<&Row> = self
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
        let ranked = matches!(filter.sort.as_str(), "top" | "suggested" | "artists");
        if ranked {
            let mut seen = HashSet::new();
            matching.retain(|r| seen.insert(r.song));
            if filter.sort == "suggested" {
                matching.retain(|r| r.score.is_some() && !on_disk.contains_key(&r.filename));
            }
            // Stable: equal scores keep the library order.
            matching.sort_by_key(|r| {
                (
                    std::cmp::Reverse(r.score.unwrap_or(0)),
                    std::cmp::Reverse(r.rank),
                )
            });
            if filter.sort == "artists" {
                // Best artist first (most Deezer fans, then the better songs), each with only their best songs.
                let fans: HashMap<&str, i64> = self
                    .state
                    .artists
                    .iter()
                    .map(|a| (a.name.as_str(), a.fans.trim().parse().unwrap_or(0)))
                    .collect();
                let mut taken: HashMap<&str, usize> = HashMap::new();
                matching.retain(|r| {
                    let n = taken.entry(r.owner.as_str()).or_default();
                    *n += 1;
                    *n <= BEST_PER_ARTIST
                });
                let best: HashMap<&str, u32> = matching.iter().fold(HashMap::new(), |mut m, r| {
                    let top = m.entry(r.owner.as_str()).or_default();
                    *top = (*top).max(r.score.unwrap_or(0));
                    m
                });
                matching.sort_by(|a, b| {
                    let (fa, fb) = (fans.get(a.owner.as_str()), fans.get(b.owner.as_str()));
                    fb.cmp(&fa)
                        .then_with(|| best[b.owner.as_str()].cmp(&best[a.owner.as_str()]))
                        .then_with(|| a.owner.cmp(&b.owner))
                        .then_with(|| {
                            (b.score.unwrap_or(0), b.rank).cmp(&(a.score.unwrap_or(0), a.rank))
                        })
                });
            }
        }
        let by_artist = filter.sort == "artists";
        let songs: HashSet<i64> = matching.iter().map(|r| r.song).collect();
        let total = songs.len();
        let linked = matching
            .iter()
            .filter(|r| !r.url.is_empty())
            .map(|r| r.song)
            .collect::<HashSet<_>>()
            .len();
        let mut albums: Vec<Album> = Vec::new();
        let mut at: HashMap<(&str, i64), usize> = HashMap::new();
        for row in matching.into_iter().take(LIMIT) {
            // A ranked list is one group (no album, id 0); by artist, one group per artist (named, id 0); else one per album.
            let group = match (ranked, by_artist) {
                (false, _) => (row.owner.as_str(), row.album_id),
                (true, false) => ("", 0),
                (true, true) => (row.owner.as_str(), 0),
            };
            let index = *at.entry(group).or_insert_with(|| {
                albums.push(if by_artist {
                    Album {
                        artist: String::new(),
                        album: row.owner.clone(),
                        year: String::new(),
                        id: 0,
                        kind: String::new(),
                        genre: String::new(),
                        tracks: Vec::new(),
                    }
                } else if ranked {
                    Album {
                        artist: String::new(),
                        album: String::new(),
                        year: String::new(),
                        id: 0,
                        kind: String::new(),
                        genre: String::new(),
                        tracks: Vec::new(),
                    }
                } else {
                    Album {
                        artist: row.owner.clone(),
                        album: row.album.clone(),
                        year: row.year.clone(),
                        id: row.album_id,
                        kind: row.kind.clone(),
                        genre: row.genre.clone(),
                        tracks: Vec::new(),
                    }
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
                score: row.score,
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
pub async fn browse(db: &mut Db, filter: &Filter) -> Result<Browse> {
    Ok(Snapshot::load(db).await?.browse(filter))
}

/// Is this a link a browser can open (`http(s)://host...`)?
#[must_use]
pub fn is_link(url: &str) -> bool {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .is_some_and(|rest| !rest.split('/').next().unwrap_or_default().is_empty())
}

/// Sets the YouTube link of a song, or clears it with an empty `url` (the next lookup will try again).
pub async fn set_link(
    db: &mut Db,
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
        .query("SELECT 1 FROM songs WHERE id = ?", &[int(song)])
        .await?
        .rows
        .is_empty()
    {
        return Ok(Err(Refused::Input("unknown track".into())));
    }
    if url.is_empty() {
        db.execute(
            "UPDATE songs SET yt_url = '', yt_channel = '', yt_title = '', yt_official = 0, yt_score = NULL, yt_views = NULL, yt_manual = 0, yt_status = NULL, yt_candidate = '' WHERE id = ?",
            &[int(song)],
        ).await?;
    } else {
        db.execute(
            "UPDATE songs SET yt_url = ?, yt_channel = 'manual', yt_title = '', yt_official = 0, yt_score = NULL, yt_views = NULL, yt_manual = 1, yt_status = 'ok', yt_candidate = '' WHERE id = ?",
            &[text(url), int(song)],
        ).await?;
    }
    Ok(Ok(1))
}

/// Blacklists the album (and its other editions) of an artist and drops it, and the songs only it held, from the index.
/// Returns how many tracks left the albums.
pub async fn blacklist_album(
    db: &mut Db,
    artist: &str,
    album_id: i64,
) -> Result<std::result::Result<u32, Refused>> {
    db.transaction(async |db| -> Result<std::result::Result<u32, Refused>> {
        let found = db.query(
            "SELECT al.title FROM albums al JOIN artists o ON o.id = al.owner_id WHERE o.listed_as = ? AND al.deezer_id = ?",
            &[text(artist), int(album_id)],
        ).await?;
        let Some(row) = found.rows.first() else {
            return Ok(Err(Refused::Input("unknown album".into())));
        };
        let title = str_at(row, 0);
        Ok(Ok(keep_out(db, artist, &title).await?))
    }).await
}

/// Blacklists `title` of `artist` and drops its editions, and the songs only they held, from the index; how many tracks left.
async fn keep_out(db: &mut Db, artist: &str, title: &str) -> Result<u32> {
    let key = album_key(title);
    follow::blacklist_add(db, artist, title).await?;
    let owned = db.query("SELECT al.id, al.title FROM albums al JOIN artists o ON o.id = al.owner_id WHERE o.listed_as = ?", &[text(artist)]).await?;
    let mut removed = 0;
    for al in owned
        .rows
        .iter()
        .filter(|r| album_key(&str_at(r, 1)) == key)
    {
        removed += songs::drop_album(db, int_at(al, 0)).await?;
    }
    songs::prune(db).await?;
    let cmap = follow::category_map(db).await?;
    songs::retag(db, &cmap).await?;
    Ok(removed)
}

/// Adds an album to the blacklist by name (`id` 0), or changes an entry (`id`): the entry takes the new artist and title and
/// what they name leaves the index now. Returns how many tracks left. An entry that is already there is refused.
pub async fn blacklist_save(
    db: &mut Db,
    id: i64,
    artist: &str,
    title: &str,
) -> Result<std::result::Result<u32, Refused>> {
    let (artist, title) = (artist.trim(), title.trim());
    if artist.is_empty() || title.is_empty() {
        return Ok(Err(Refused::Input(
            "an artist and an album are needed".into(),
        )));
    }
    db.transaction(async |db| -> Result<std::result::Result<u32, Refused>> {
        if id != 0 && !follow::blacklist_remove(db, id).await? {
            return Ok(Err(Refused::Input("unknown entry".into())));
        }
        let before = follow::blacklist(db).await?.len();
        let removed = keep_out(db, artist, title).await?;
        if id == 0 && follow::blacklist(db).await?.len() == before {
            return Ok(Err(Refused::Input("it is on the blacklist already".into())));
        }
        Ok(Ok(removed))
    })
    .await
}

/// Songs by a followed artist (the name as followed or as Deezer spells it), as song ids.
pub async fn songs_of(db: &mut Db, artist: &str) -> Result<HashSet<i64>> {
    let rows = db.query(
        "SELECT sa.song_id FROM song_artist sa JOIN artists a ON a.id = sa.artist_id WHERE a.name = ? OR a.listed_as = ?",
        &[text(artist), text(artist)],
    ).await?;
    Ok(rows.rows.iter().map(|r| int_at(r, 0)).collect())
}
