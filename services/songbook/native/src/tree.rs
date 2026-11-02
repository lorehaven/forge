//! What Android Auto (and the phone's browser) sees of the library.
//!
//! A tree of folders and songs, made from the API when asked for. Every id says where it is: `artist/Nate%20Dogg`, `album/7`; a song's is `s:<place>:<song>`, so that choosing it can
//! play the songs of its place, not just that one. (The folder of kept songs, `kept`, is the shell's: it knows the device.)

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use quench_serde::Serialize;
use songbook_core::api::encode;
use songbook_core::{AlbumInfo, ArtistInfo, Home, Overview, SongInfo, SongQuery};

use crate::api::{Api, Result};
use crate::store::Store;

pub const ROOT: &str = "root";
pub const RECENT_ROOT: &str = "recent-root";
pub const SUGGESTED_ROOT: &str = "suggested-root";

/// How long what was asked of the service is used again (the car asks often).
const FRESH: Duration = Duration::from_secs(120);
/// A car shows a screenful: a place's queue is not the whole library.
const CAR_LIMIT: u32 = 300;
/// How many songs the phone remembers.
const REMEMBERED: usize = 40;

/// A folder or a song, as the shell turns it into a `MediaItem`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub album: String,
    /// Artwork address; empty for none.
    pub art: String,
    /// What to play; empty for a folder.
    pub uri: String,
    pub duration_ms: i64,
    pub playable: bool,
    pub browsable: bool,
    /// `folder`, `album`, `artist`, `playlist`, `artists`, `playlists` or `song`.
    pub kind: String,
}

/// The queue to play when a song is chosen in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Queue {
    pub items: Vec<Item>,
    pub start: usize,
    pub shuffled: bool,
}

pub struct Tree {
    api: Arc<dyn Api>,
    store: Store,
    home: Mutex<Option<(Instant, Home)>>,
    overview: Mutex<Option<(Instant, Overview)>>,
}

impl std::fmt::Debug for Tree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Tree { .. }")
    }
}

fn cached<T: Clone>(
    slot: &Mutex<Option<(Instant, T)>>,
    make: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let held = slot
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|(at, v)| (at.elapsed() < FRESH).then(|| v.clone()));
    if let Some(value) = held {
        return Ok(value);
    }
    let value = make()?;
    *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some((Instant::now(), value.clone()));
    Ok(value)
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                if let Some(b) = hex {
                    out.push(b);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn letter(name: &str) -> String {
    name.trim().chars().next().map_or_else(
        || "#".into(),
        |c| {
            if c.is_alphabetic() {
                c.to_uppercase().collect()
            } else {
                "#".into()
            }
        },
    )
}

/// `s:<place>:<id>` as (place, id).
fn parse_song(id: &str) -> Option<(&str, i64)> {
    let mut parts = id.splitn(3, ':');
    (parts.next()? == "s").then_some(())?;
    let place = parts.next()?;
    Some((place, parts.next()?.parse().ok()?))
}

fn folder(id: &str, title: &str, subtitle: &str, art: &str, kind: &str) -> Item {
    Item {
        id: id.into(),
        title: title.into(),
        subtitle: subtitle.into(),
        artist: String::new(),
        album: String::new(),
        art: art.into(),
        uri: String::new(),
        duration_ms: 0,
        playable: false,
        browsable: true,
        kind: kind.into(),
    }
}

impl Tree {
    #[must_use]
    pub fn new(api: Arc<dyn Api>, store: Store) -> Self {
        Self {
            api,
            store,
            home: Mutex::new(None),
            overview: Mutex::new(None),
        }
    }

    fn home(&self) -> Result<Home> {
        cached(&self.home, || self.api.home())
    }

    fn overview(&self) -> Result<Overview> {
        cached(&self.overview, || self.api.overview())
    }

    /// What was played last: as the service knows it (every device), else what this phone remembers.
    fn recent_songs(&self) -> Vec<SongInfo> {
        let local = self.store.recent();
        match self.api.recent() {
            Ok(mut server) => {
                let extra: Vec<SongInfo> = local
                    .into_iter()
                    .filter(|l| server.iter().all(|s| s.id != l.id))
                    .collect();
                server.extend(extra);
                server
            }
            Err(_) => local,
        }
    }

    // ── Items ────────────────────────────────────────────────────────────────

    #[must_use]
    pub fn stream_uri(&self, id: i64) -> String {
        format!("{}/stream/{id}", self.api.base())
    }

    fn song_item(&self, place: &str, s: &SongInfo) -> Item {
        let base = self.api.base();
        Item {
            id: format!("s:{place}:{}", s.id),
            title: s.title.clone(),
            subtitle: String::new(),
            artist: s.artists.join(", "),
            album: s.album.clone(),
            art: format!("{base}/art/song/{}?w=320", s.id),
            uri: self.stream_uri(s.id),
            duration_ms: i64::from(s.duration) * 1000,
            playable: true,
            browsable: false,
            kind: "song".into(),
        }
    }

    fn song_items(&self, place: &str, songs: &[SongInfo]) -> Vec<Item> {
        songs.iter().map(|s| self.song_item(place, s)).collect()
    }

    fn album_folder(&self, a: &AlbumInfo) -> Item {
        let year: String = a.year.chars().take(4).collect();
        let subtitle = format!("{year} · {}", a.artist);
        let subtitle = subtitle.trim_matches(|c| c == ' ' || c == '·');
        folder(
            &format!("album/{}", a.id),
            &a.title,
            subtitle,
            &format!("{}/art/album/{}?w=320", self.api.base(), a.id),
            "album",
        )
    }

    fn artist_folder(&self, a: &ArtistInfo) -> Item {
        let art = format!(
            "{}/art/artist?name={}&w=320",
            self.api.base(),
            encode(&a.name)
        );
        folder(
            &format!("artist/{}", encode(&a.name)),
            &a.name,
            &format!("{} songs", a.songs),
            &art,
            "artist",
        )
    }

    // ── The tree ─────────────────────────────────────────────────────────────

    #[must_use]
    pub fn root_children() -> Vec<Item> {
        vec![
            folder("recent", "Recently played", "", "", "folder"),
            folder("mixes", "Mixes", "", "", "folder"),
            folder("popular", "Popular", "", "", "folder"),
            folder("artists", "Artists", "", "", "artists"),
            folder("categories", "Categories", "", "", "folder"),
            folder("playlists", "Playlists", "", "", "playlists"),
        ]
    }

    /// The items of a folder.
    ///
    /// # Errors
    /// The service could not be asked.
    pub fn children(&self, parent: &str) -> Result<Vec<Item>> {
        let (kind, key) = parent.split_once('/').unwrap_or((parent, ""));
        Ok(match parent {
            ROOT => Self::root_children(),
            "recent" => self.song_items("recent", &self.recent_songs()),
            // What Android Auto asks for to offer "continue": the song that was playing, and a few to start with.
            RECENT_ROOT => match self.store.last() {
                Some(last) => vec![self.song_item("recent", &last.song)],
                None => self.song_items(
                    "recent",
                    &self.recent_songs().into_iter().take(1).collect::<Vec<_>>(),
                ),
            },
            SUGGESTED_ROOT => self.song_items(
                "popular",
                &self.home()?.popular.into_iter().take(5).collect::<Vec<_>>(),
            ),
            "popular" => self.song_items("popular", &self.home()?.popular),
            "mixes" => self
                .home()?
                .mixes
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let sub = if m.shuffled() {
                        format!("Shuffled · {} songs", m.songs)
                    } else {
                        format!("Top {}", m.songs)
                    };
                    folder(&format!("mix/{i}"), &m.name, &sub, "", "folder")
                })
                .collect(),
            "artists" => {
                let mut letters: Vec<String> = self
                    .overview()?
                    .artists
                    .iter()
                    .map(|a| letter(&a.name))
                    .collect();
                letters.sort();
                letters.dedup();
                letters
                    .iter()
                    .map(|l| folder(&format!("letter/{l}"), l, "", "", "folder"))
                    .collect()
            }
            "categories" => self
                .overview()?
                .categories
                .iter()
                .map(|c| {
                    folder(
                        &format!("category/{}", encode(&c.name)),
                        &c.name,
                        &format!("{} songs", c.songs),
                        "",
                        "folder",
                    )
                })
                .collect(),
            "playlists" => self
                .api
                .playlists()?
                .iter()
                .map(|p| {
                    folder(
                        &format!("playlist/{}", p.id),
                        &p.name,
                        &format!("{} songs", p.songs),
                        "",
                        "playlist",
                    )
                })
                .collect(),
            _ if kind == "letter" => self
                .overview()?
                .artists
                .iter()
                .filter(|a| letter(&a.name) == key)
                .map(|a| self.artist_folder(a))
                .collect(),
            _ if kind == "artist" => {
                let name = decode(key);
                let mut items = vec![folder(
                    &format!("artistsongs/{}", encode(&name)),
                    "All songs",
                    "",
                    "",
                    "folder",
                )];
                items.extend(self.api.albums(&name)?.iter().map(|a| self.album_folder(a)));
                items
            }
            _ => {
                // `artistsongs/x` lists the songs of `artist/x`.
                let place = if kind == "artistsongs" {
                    format!("artist/{key}")
                } else {
                    parent.to_string()
                };
                self.song_items(&place, &self.songs_of(&place)?.0)
            }
        })
    }

    /// The songs of a place, and whether it is played shuffled (a mix).
    fn songs_of(&self, place: &str) -> Result<(Vec<SongInfo>, bool)> {
        let (kind, key) = place.split_once('/').unwrap_or((place, ""));
        let query = |q: SongQuery| self.api.songs(&q);
        Ok(match kind {
            "recent" => (self.recent_songs(), false),
            "popular" => (self.home()?.popular, false),
            "mix" => match self
                .home()?
                .mixes
                .get(key.parse::<usize>().unwrap_or(usize::MAX))
            {
                Some(m) => (query(m.query())?, m.shuffled()),
                None => (Vec::new(), false),
            },
            "album" => (
                query(SongQuery {
                    album: key.parse().unwrap_or(0),
                    ..SongQuery::default()
                })?,
                false,
            ),
            "artist" => (
                query(SongQuery {
                    artist: decode(key),
                    limit: CAR_LIMIT,
                    ..SongQuery::default()
                })?,
                false,
            ),
            "category" => (
                query(SongQuery {
                    category: decode(key),
                    sort: "popular".into(),
                    limit: 100,
                    ..SongQuery::default()
                })?,
                false,
            ),
            "playlist" => (
                query(SongQuery {
                    playlist: key.parse().unwrap_or(0),
                    limit: CAR_LIMIT,
                    ..SongQuery::default()
                })?,
                false,
            ),
            "search" => (self.api.search(&decode(key))?.songs, false),
            _ => (Vec::new(), false),
        })
    }

    /// A folder or a song by its id; `None` for one that is not there.
    ///
    /// # Errors
    /// The service could not be asked.
    pub fn item(&self, id: &str) -> Result<Option<Item>> {
        if id == ROOT {
            return Ok(Some(folder(ROOT, "Songbook", "", "", "folder")));
        }
        let Some((place, song)) = parse_song(id) else {
            return Ok(None);
        };
        Ok(self
            .songs_of(place)?
            .0
            .iter()
            .find(|s| s.id == song)
            .map(|s| self.song_item(place, s)))
    }

    /// The queue for a chosen song: its place's songs, started at it. `None` when the id is not a song of this tree.
    ///
    /// # Errors
    /// The service could not be asked.
    pub fn queue(&self, id: &str) -> Result<Option<Queue>> {
        let Some((place, song)) = parse_song(id) else {
            return Ok(None);
        };
        let (songs, shuffled) = self.songs_of(place)?;
        if songs.is_empty() {
            return Ok(None);
        }
        let start = songs.iter().position(|s| s.id == song).unwrap_or(0);
        Ok(Some(Queue {
            items: self.song_items(place, &songs),
            start,
            shuffled,
        }))
    }

    /// The songs with these ids, in this order, as the player's queue (place `q`, which has no songs of its own: choosing one in
    /// a car does not replace the queue).
    ///
    /// # Errors
    /// The service could not be asked.
    pub fn songs(&self, ids: &[i64]) -> Result<Vec<Item>> {
        let list = ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let found = self.api.songs(&SongQuery {
            ids: list,
            limit: u32::try_from(ids.len()).unwrap_or(u32::MAX),
            ..SongQuery::default()
        })?;
        Ok(self.song_items("q", &found))
    }

    /// The songs for words said or typed (the voice search of the car).
    ///
    /// # Errors
    /// The service could not be asked.
    pub fn search(&self, text: &str) -> Result<Vec<Item>> {
        Ok(self.song_items(
            &format!("search/{}", encode(text)),
            &self.api.search(text)?.songs,
        ))
    }

    /// A song was started: kept on this phone, and told to the service (which may be out of reach; the phone remembers).
    pub fn played(&self, song: &SongInfo) {
        self.store.add_recent(song, REMEMBERED);
        let _ = self.api.played(song.id);
    }
}
