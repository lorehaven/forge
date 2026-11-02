//! The JSON the listener API speaks: what the player reads and keeps. Every endpoint is under `/api/v1` (not
//! `/api/v1/admin`) and needs any grant of `songbook`.
//!
//! | Method and path                       | Body / query          | Answer          |
//! |---------------------------------------|-----------------------|-----------------|
//! | `GET  /library/overview`              |                       | [`Overview`]    |
//! | `GET  /library/albums`                | [`AlbumQuery`]        | a list of [`AlbumInfo`] |
//! | `GET  /library/home`                  |                       | [`Home`]        |
//! | `GET  /library/recent?limit=`         |                       | [`SongList`] (what this person played last, newest first) |
//! | `POST /history`                       | [`PlayedBody`]        | `{}`            |
//! | `GET  /library/songs`                 | [`SongQuery`]         | [`SongList`]    |
//! | `GET  /library/ids`                   | [`SongQuery`]         | [`SongIds`]     |
//! | `GET  /library/search?q=`             |                       | [`Search`]      |
//! | `GET  /stream/{song}`                 | `Range` header        | the audio file  |
//! | `GET  /playlists`                     |                       | a list of [`PlaylistInfo`] |
//! | `POST /playlists`                     | [`PlaylistBody`]      | [`PlaylistInfo`] |
//! | `PUT  /playlists/{id}`                | [`PlaylistBody`]      | [`PlaylistInfo`] |
//! | `DELETE /playlists/{id}`              |                       | `{}`            |
//! | `POST /playlists/{id}/songs`          | [`SongIdsBody`]       | [`PlaylistInfo`] |
//! | `PUT  /playlists/{id}/songs`          | [`SongIdsBody`]       | [`PlaylistInfo`] (the order replaced) |
//!
//! Only songs that have a file are listed: the library the listener sees is what can be played.

use quench_serde::{Deserialize, Serialize};

use crate::api::encode;

/// `?a=..&b=..` from the pairs that are set; empty when none is.
fn query_of(pairs: &[(&str, String)]) -> String {
    let parts: Vec<String> = pairs
        .iter()
        .filter(|(_, v)| !v.is_empty() && v != "0")
        .map(|(k, v)| format!("{k}={}", encode(v)))
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

/// One song that can be played.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongInfo {
    pub id: i64,
    pub title: String,
    /// Main artist first.
    pub artists: Vec<String>,
    /// The album it is first found on (empty for none).
    pub album: String,
    pub album_id: i64,
    pub year: String,
    /// Seconds; 0 when unknown.
    pub duration: u32,
}

/// An album with at least one playable song.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumInfo {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub year: String,
    /// `album`, `single`, `ep`, ... as Deezer says.
    pub kind: String,
    /// Playable songs on it.
    pub songs: u32,
}

/// An artist with at least one playable song (credited on it, not only the owner of its album).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistInfo {
    pub name: String,
    pub songs: u32,
    pub albums: u32,
}

/// A category of the followed artists (`Rap`, `Rock`, ...), with its playable songs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryInfo {
    pub name: String,
    pub songs: u32,
}

/// The home page: what there is.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Overview {
    pub songs: u32,
    pub albums: u32,
    pub artists: Vec<ArtistInfo>,
    pub categories: Vec<CategoryInfo>,
}

/// Which songs: the first field that is set picks the list (`ids`, then `playlist`, `album`, `artist`, `category`, `q`);
/// with none, every song.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongQuery {
    /// Song ids, `,`-separated, answered in this order.
    #[serde(default)]
    pub ids: String,
    #[serde(default)]
    pub playlist: i64,
    #[serde(default)]
    pub album: i64,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub category: String,
    /// Words to find in a title, album or artist name.
    #[serde(default)]
    pub q: String,
    /// `popular` puts the most popular first (before `limit` cuts); empty keeps the list's own order.
    #[serde(default)]
    pub sort: String,
    /// At most this many (0: the endpoint's own limit).
    #[serde(default)]
    pub limit: u32,
}

impl SongQuery {
    /// `?album=..&q=..` with the unset fields left out.
    #[must_use]
    pub fn query(&self) -> String {
        query_of(&[
            ("ids", self.ids.clone()),
            ("playlist", self.playlist.to_string()),
            ("album", self.album.to_string()),
            ("artist", self.artist.clone()),
            ("category", self.category.clone()),
            ("q", self.q.clone()),
            ("sort", self.sort.clone()),
            ("limit", self.limit.to_string()),
        ])
    }
}

/// The songs asked for, in play order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongList {
    pub songs: Vec<SongInfo>,
    /// How many there are (more than `songs.len()` when it was cut).
    pub total: u32,
}

/// The ids only: what a queue is made of, so a whole category fits in a few kilobytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongIds {
    pub ids: Vec<i64>,
    pub total: u32,
}

/// The albums asked for (of an artist, a category, or found by words).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumQuery {
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub limit: u32,
}

impl AlbumQuery {
    /// `?artist=..` with the unset fields left out.
    #[must_use]
    pub fn query(&self) -> String {
        query_of(&[
            ("artist", self.artist.clone()),
            ("category", self.category.clone()),
            ("q", self.q.clone()),
            ("limit", self.limit.to_string()),
        ])
    }
}

/// Something made for the person to play: the popular songs of a category (or of everything), in order or shuffled. It is
/// not stored: playing it asks for [`Mix::query`] and plays what comes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mix {
    /// What the card says (`Polish Rap Mix`, `Best of Polish Rap`).
    pub name: String,
    /// `mix` (shuffled) or `best` (the most popular first).
    pub kind: String,
    /// The category it is made from; empty for the whole library.
    pub category: String,
    /// How many songs it plays.
    pub songs: u32,
}

impl Mix {
    /// The songs to ask for to play it.
    #[must_use]
    pub fn query(&self) -> SongQuery {
        SongQuery {
            category: self.category.clone(),
            sort: "popular".into(),
            limit: self.songs,
            ..SongQuery::default()
        }
    }

    /// Whether it is played shuffled.
    #[must_use]
    pub fn shuffled(&self) -> bool {
        self.kind == "mix"
    }
}

/// The home page: what to listen to, chosen from the library. The picks change every day.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Home {
    /// The most popular songs, at most two of an artist.
    pub popular: Vec<SongInfo>,
    pub mixes: Vec<Mix>,
    /// Artists to try: popular ones, a different few each day.
    pub artists: Vec<ArtistInfo>,
    /// Songs worth hearing again: well known, but not the first that come to mind.
    pub discover: Vec<SongInfo>,
    /// The newest albums.
    pub albums: Vec<AlbumInfo>,
}

/// A song the person has just started: what `POST /history` is told.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayedBody {
    pub song_id: i64,
}

/// The query of `GET /library/recent`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentQuery {
    /// At most this many (0: 40).
    #[serde(default)]
    pub limit: u32,
}

/// What the search box finds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Search {
    pub songs: Vec<SongInfo>,
    pub albums: Vec<AlbumInfo>,
    pub artists: Vec<ArtistInfo>,
}

/// The query of `GET /library/search`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
}

/// A playlist of the signed-in person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistInfo {
    pub id: i64,
    pub name: String,
    /// Entries in it.
    pub songs: u32,
}

/// The body that makes or renames a playlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistBody {
    pub name: String,
}

/// Song ids to add to a playlist, or (on `PUT`) the whole new order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongIdsBody {
    pub song_ids: Vec<i64>,
}
