//! The JSON the admin API speaks. Every endpoint is under `/api/v1/admin` and needs a signed-in session.
//!
//! | Method and path                   | Body / query        | Answer      |
//! |-----------------------------------|---------------------|-------------|
//! | `GET  /state`                     |                     | [`State`]   |
//! | `GET  /browse`                    | [`Filter`] as query | [`Browse`]  |
//! | `GET  /stats`                     |                     | [`Stats`]   |
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
    /// Only albums in these genres are kept (Deezer's names, `; `-separated); empty keeps every album.
    #[serde(default)]
    pub genres: String,
    /// Songs of the artist's own discography (the albums found under them), each once: what the library page counts for them.
    pub tracks: u32,
    /// The last sync found this artist.
    pub synced: bool,
}

/// One of an artist's best songs, as Deezer ranks them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopTrack {
    pub title: String,
    pub album: String,
    /// Popularity 0-100 (see [`popularity`]).
    pub score: u32,
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
    /// Their best songs on Deezer, best first; empty when not fetched.
    #[serde(default)]
    pub best: Vec<TopTrack>,
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

/// How big the library is and how much of it is on disk: the numbers on the main page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    /// Followed artists.
    pub artists: u32,
    /// Every artist in the index, followed or only credited on a song.
    pub credited: u32,
    pub albums: u32,
    pub songs: u32,
    /// Songs with a file in the audio folder, in any format.
    pub downloaded: u32,
    /// Songs with no file yet.
    pub missing: u32,
    /// Of the missing, the ones that have a link to download from.
    pub missing_linked: u32,
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
    /// Popularity 0-100 from Deezer's rank and the video's `YouTube` views (see [`popularity`]); `None` when neither is known.
    #[serde(default)]
    pub score: Option<u32>,
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
    /// `top` (most popular songs first), `artists` (artists with most fans first, each with their best songs) or `suggested` (most popular not yet downloaded); empty for the library order.
    #[serde(default)]
    pub sort: String,
}

/// Popularity of a song, 0-100.
///
/// Built from Deezer's track `rank` (0 to about 1,000,000) and the views of its `YouTube` video (log scale, 1,000,000,000
/// views is the top). Both known: 60% Deezer, 40% `YouTube`; one known: that one; none: `None`.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // 0..=100 from clamped ratios
pub fn popularity(rank: Option<i64>, views: Option<i64>) -> Option<u32> {
    let deezer = rank
        .filter(|r| *r > 0)
        .map(|r| (r as f64 / 1_000_000.0).min(1.0));
    let youtube = views
        .filter(|v| *v > 0)
        .map(|v| ((v as f64).ln_1p() / 1e9_f64.ln_1p()).min(1.0));
    let blended = match (deezer, youtube) {
        (Some(d), Some(y)) => 0.4_f64.mul_add(y, 0.6 * d),
        (Some(x), None) | (None, Some(x)) => x,
        (None, None) => return None,
    };
    Some((blended * 100.0).round() as u32)
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
            ("sort", &self.sort),
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

/// One album kept out of every sync (`GET /blacklist`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlacklistEntry {
    pub id: i64,
    pub artist: String,
    pub title: String,
}

/// `POST /blacklist`: add an album to the blacklist (`id` 0), or change an entry (its `id`). The album, and the songs only it
/// held, leave the index at once, as when it is blacklisted from the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlacklistSave {
    #[serde(default)]
    pub id: i64,
    pub artist: String,
    pub title: String,
}

/// `POST /blacklist/remove`: let an album back in (the next sync brings it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlacklistRemove {
    pub id: i64,
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

/// `POST /artists/genres`: the genres an artist's albums must have to be kept (`; `-separated), empty for any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenresBody {
    pub name: String,
    pub genres: String,
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

/// `POST /jobs`: which job to start, and its options.
///
/// `name` is `sync`, `tags`, `similar`, `youtube`, `download`, `convert`, `images` (pictures of artists and albums;
/// `retry_missing` asks again for those Deezer had none for), `purge` (the files of blacklisted albums), `pipeline` or
/// `scheduled` (the stages of the saved schedule, now).
///
/// `pipeline` is sync, find links (with the songs that had no match tried again), download, convert and find images, one after the other
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
    /// purge: only list the files that would be deleted.
    #[serde(default)]
    pub dry_run: bool,
}

/// The nightly run (`GET`/`POST /schedule`).
///
/// At `time` (`HH:MM`, in `zone`) every day the chosen stages run one after the other as one job. A stage that fails does
/// not stop the next ones. The last four fields are filled by the service; what a client sends in them is ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // one switch per stage and option, as the page shows them
pub struct Schedule {
    pub enabled: bool,
    /// `HH:MM`, 24-hour.
    pub time: String,
    /// An IANA zone name such as `Europe/Warsaw`; empty for the service's own zone.
    #[serde(default)]
    pub zone: String,
    /// Stage 1: sync with Deezer.
    pub sync: bool,
    /// Stage 2: find `YouTube` links, trying again the songs that had no match.
    pub links: bool,
    /// Stage 3: download every song that has a link and no file yet (the ones that failed before are tried again).
    pub download: bool,
    /// Stage 4: convert the downloads to mp3.
    pub convert: bool,
    /// Stage 5: refresh the suggested artists and their best songs. A schedule saved before this stage existed has it off.
    #[serde(default)]
    pub suggestions: bool,
    /// Stage 6: find the pictures of artists, albums and singles (what is missing only). A schedule saved before this stage
    /// existed has it off.
    #[serde(default)]
    pub images: bool,
    /// Sync asks Deezer for every track again instead of using what it saved, so the ratings are current.
    pub fresh_ratings: bool,
    /// Sync includes singles.
    #[serde(default)]
    pub singles: bool,
    /// The local day the schedule last started (`YYYY-MM-DD`); it runs once a day.
    #[serde(default)]
    pub last_day: String,
    /// When it last started, `YYYY-MM-DD HH:MM` in `zone`; empty before the first run.
    #[serde(default)]
    pub last_run: String,
    /// `ok`, or why the last run did not finish well.
    #[serde(default)]
    pub last_result: String,
    /// When it runs next, `YYYY-MM-DD HH:MM` in `zone`; empty when it is off.
    #[serde(default)]
    pub next_run: String,
    /// The zone in force (`zone`, or the service's own when that is empty).
    #[serde(default)]
    pub zone_in_use: String,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            enabled: false,
            time: "03:00".into(),
            zone: String::new(),
            sync: true,
            links: true,
            download: true,
            convert: true,
            suggestions: true,
            images: true,
            fresh_ratings: true,
            singles: false,
            last_day: String::new(),
            last_run: String::new(),
            last_result: String::new(),
            next_run: String::new(),
            zone_in_use: String::new(),
        }
    }
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
