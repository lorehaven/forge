//! The JSON the admin API speaks. Every endpoint is under `/api/v1/admin` and needs a signed-in session.
//!
//! | Method and path                   | Body / query        | Answer      |
//! |-----------------------------------|---------------------|-------------|
//! | `GET  /state`                     |                     | [`State`]   |
//! | `GET  /browse`                    | [`Filter`] as query | [`Browse`]  |
//! | `POST /link`                      | [`LinkBody`]        | [`Updated`] |
//! | `POST /albums/blacklist`          | [`BlacklistBody`]   | [`Removed`] |
//! | `POST /artists/add`               | [`AddArtists`]      | [`Added`]   |
//! | `POST /artists/remove`            | [`RemoveArtist`]    | `{}`        |
//! | `POST /artists/category`          | [`CategoryBody`]    | [`Retagged`]|
//! | `POST /categories/add`            | [`NewCategory`]     | [`Added`]   |
//! | `POST /categories/rename`         | [`RenameCategory`]  | [`Retagged`]|
//! | `POST /jobs`                      | [`JobStart`]        | [`Started`] |
//! | `GET  /job?from=N`                |                     | [`Job`]     |
//!
//! Failures are `{"error": "..."}` with 400 (bad input), 409 (a job is running) or 401 (signed out).

use quench_serde::{Deserialize, Serialize};

/// A followed artist, as the sidebar shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistRow {
    /// The name as followed (what `artist` filters and category/remove calls use).
    pub name: String,
    /// The Deezer id was given by hand.
    pub pinned: bool,
    /// A category from `State::categories`, or `State::uncategorized`.
    pub category: String,
    /// Who Deezer matched; empty before the first sync.
    pub deezer_name: String,
    pub deezer_id: String,
    pub fans: String,
    /// Songs of the artist's own discography (the albums found under them), each once: what the library page counts for them.
    pub tracks: u32,
    /// The last sync found this artist.
    pub synced: bool,
}

/// An artist `similar` suggests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub artist: String,
    pub deezer_id: String,
    pub fans: String,
    pub related_to_count: u32,
    /// Followed artists that point at it, `; `-separated.
    pub related_to: String,
    /// The category the list was made for; empty for all.
    pub scope: String,
}

/// Everything the sidebar and the filters need.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub artists: Vec<ArtistRow>,
    pub categories: Vec<String>,
    /// The name of the category of artists with none (`Uncategorized`).
    pub uncategorized: String,
    pub genres: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}

/// One song on one album.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    /// The song's id in the index (the same on every album it is on).
    pub id: i64,
    pub title: String,
    /// All artists, `, `-separated, main artist first.
    pub artists: String,
    /// The `YouTube` link; empty when there is none.
    pub url: String,
    /// The link is on an official/artist channel.
    pub official: bool,
    /// The link was set by hand.
    pub manual: bool,
}

/// An album with its tracklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Album {
    /// The followed artist the album was found under.
    pub artist: String,
    pub album: String,
    pub year: String,
    /// The Deezer album id.
    pub id: i64,
    /// `album`, `ep` or `single`.
    pub kind: String,
    /// Deezer's genres, `; `-separated.
    pub genre: String,
    pub tracks: Vec<Track>,
}

/// What the library page shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Browse {
    /// Songs that match the filter, each once (a song on three albums is listed three times but counts once).
    pub total: u32,
    /// Of those, the ones with a link.
    pub linked: u32,
    /// How many tracks are listed in `albums` (at most the limit; a song on several albums is listed on each).
    pub shown: u32,
    pub albums: Vec<Album>,
}

/// The library filter, as the query of `GET /browse`. Empty means no filter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub genre: String,
    /// Words to find in a title, album or artist name.
    #[serde(default)]
    pub q: String,
    /// `has`, `missing` or `unofficial`; empty for all.
    #[serde(default)]
    pub links: String,
}

impl Filter {
    /// `?artist=..&q=..` with the empty fields left out; empty string when nothing is set.
    #[must_use]
    pub fn query(&self) -> String {
        let pairs = [
            ("artist", &self.artist),
            ("category", &self.category),
            ("genre", &self.genre),
            ("q", &self.q),
            ("links", &self.links),
        ];
        let parts: Vec<String> = pairs
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| format!("{k}={}", encode(v)))
            .collect();
        if parts.is_empty() {
            String::new()
        } else {
            format!("?{}", parts.join("&"))
        }
    }
}

/// Percent-encodes a query value or path segment.
#[must_use]
pub fn encode(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// `POST /link`: set the `YouTube` link of a song, or clear it with an empty `url`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkBody {
    pub track_id: i64,
    pub url: String,
}

/// `POST /albums/blacklist`: skip this album in every later sync and drop it from the index now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlacklistBody {
    pub artist: String,
    pub album_id: i64,
}

/// An artist to follow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewArtist {
    pub name: String,
    /// Deezer artist id to pin; empty to match by name.
    #[serde(default)]
    pub id: String,
    /// An existing category; empty for none.
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddArtists {
    pub items: Vec<NewArtist>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveArtist {
    pub name: String,
}

/// `POST /artists/category`: `category` empty for none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryBody {
    pub name: String,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewCategory {
    pub name: String,
}

/// `POST /categories/rename`: the artists keep their place; their songs are retagged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameCategory {
    pub name: String,
    pub new_name: String,
}

/// `POST /jobs`: `name` is `sync`, `tags`, `similar`, `youtube`, `download`, `convert` or `pipeline`.
///
/// `pipeline` is sync, find links (with the songs that had no match tried again), download and convert, one after the other
/// in one job; it stops at the first step that fails.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobStart {
    pub name: String,
    /// sync: include singles.
    #[serde(default)]
    pub singles: bool,
    /// youtube, download: only songs by this followed artist; empty for all.
    #[serde(default)]
    pub artist: String,
    /// youtube, download: at most this many songs. (pipeline: the limit of find links.)
    #[serde(default)]
    pub limit: Option<u32>,
    /// pipeline: at most this many songs downloaded; `limit` is for find links.
    #[serde(default)]
    pub limit_download: Option<u32>,
    /// similar: how many suggestions to keep (default 50).
    #[serde(default)]
    pub top: Option<u32>,
    /// similar: related artists asked per followed artist (default 20).
    #[serde(default)]
    pub per_artist: Option<u32>,
    /// similar: only use artists of this category as seeds; empty for all.
    #[serde(default)]
    pub category: String,
    /// youtube: look again at songs that had no match.
    #[serde(default)]
    pub retry_missing: bool,
}

/// The one job: running now, or the last one that ran.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub name: Option<String>,
    pub running: bool,
    pub error: Option<String>,
    /// The log from character `from` (the query of `GET /job`) to its end.
    pub log: String,
    /// Where the log ends now: the `from` of the next poll. It restarts from 0 when a new job starts, which the client
    /// sees as `offset < from`.
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Started {
    /// `false` when a job is already running.
    pub started: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Updated {
    pub updated: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Removed {
    pub removed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retagged {
    pub retagged: u32,
}

/// Names that were new.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Added {
    pub added: Vec<String>,
}
