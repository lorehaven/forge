//! The Deezer API: throttled, retried, with the album and track answers cached on disk (the same files the Python helper
//! wrote, so its cache is reused).

use quench_core::async_trait;
use quench_serde::{Value, json};
use songbook_core::naming::norm;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};

use crate::index::songs::AlbumInfo;

/// One GET against Deezer: `Ok(value)` for an answer (an API error object is returned as is), `Err` when it could not be
/// reached or read. The real one is [`Http`]; tests use a table of canned answers.
#[async_trait]
pub trait Transport: Send + Sync {
    async fn get(&self, path: &str, params: &[(String, String)]) -> Result<Value, String>;
}

/// The Deezer API over HTTPS.
#[derive(Debug, Clone)]
pub struct Http {
    client: quench_client::transport::Client,
    base: String,
}

impl Http {
    #[must_use]
    pub fn new(base: &str) -> Self {
        let client = quench_client::transport::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("songbook/1.0")
            .build()
            .unwrap_or_default();
        Self {
            client,
            base: base.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl Transport for Http {
    async fn get(&self, path: &str, params: &[(String, String)]) -> Result<Value, String> {
        let response = self
            .client
            .get(format!("{}/{path}", self.base))
            .query(params)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body = response.text().await.map_err(|e| e.to_string())?;
        json::parse(&body).map_err(|e| e.to_string())
    }
}

/// An integer field of an object, whatever integer type the parser chose.
#[must_use]
pub fn int_of(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(Value::as_i64)
}

#[must_use]
pub fn str_of(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// An artist as `search/artist` and `artist/{id}` give it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistHit {
    pub id: i64,
    pub name: String,
    pub fans: i64,
}

fn artist_hit(v: &Value) -> Option<ArtistHit> {
    Some(ArtistHit {
        id: int_of(v, "id")?,
        name: str_of(v, "name"),
        fans: int_of(v, "nb_fan").unwrap_or(0),
    })
}

/// A track of an album's tracklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRef {
    pub id: i64,
    pub title: String,
    pub duration: Option<i64>,
    /// The track's own artist (the album tracklist names one).
    pub artist: Option<(i64, String)>,
}

/// An album with its tracklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumDetail {
    pub info: AlbumInfo,
    pub tracks: Vec<TrackRef>,
}

/// What `track/{id}` adds: every contributor, not just the album artist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackDetail {
    pub title: String,
    pub duration: Option<i64>,
    pub contributors: Vec<(i64, String)>,
    /// Deezer's popularity score of the track (0 to about 1,000,000).
    pub rank: Option<i64>,
}

/// One of an artist's best songs (`artist/{id}/top`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistTrack {
    pub id: i64,
    pub title: String,
    pub album: String,
    /// Deezer's popularity score of the track.
    pub rank: i64,
}

/// An album listed under an artist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumRef {
    pub id: i64,
    pub title: String,
    pub record_type: String,
    pub release_date: String,
    pub explicit: bool,
}

/// A related artist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Related {
    pub id: i64,
    pub name: String,
    pub fans: i64,
}

#[derive(Clone)]
pub struct Deezer {
    transport: Arc<dyn Transport>,
    cache: PathBuf,
    last: Arc<Mutex<Instant>>,
    details: Arc<Semaphore>,
    /// Minimum time between two requests (Deezer allows roughly 50 per 5 seconds).
    gap: Duration,
    /// How long to wait after a transport error or a quota answer before the first retry.
    pause: Duration,
    /// Saved track files older than this are fetched again (`None`: they never expire). Ratings change.
    track_ttl: Option<Duration>,
}

impl std::fmt::Debug for Deezer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Deezer { .. }")
    }
}

impl Deezer {
    #[must_use]
    pub fn new(transport: Arc<dyn Transport>, cache: PathBuf) -> Self {
        Self {
            transport,
            cache,
            last: Arc::new(Mutex::new(
                Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .unwrap_or_else(Instant::now),
            )),
            details: Arc::new(Semaphore::new(4)),
            gap: Duration::from_millis(150),
            pause: Duration::from_secs(2),
            track_ttl: None,
        }
    }

    /// The real API, caching under `cache`.
    #[must_use]
    pub fn live(cache: PathBuf) -> Self {
        Self::new(Arc::new(Http::new("https://api.deezer.com")), cache)
    }

    /// The same client, but saved track files older than `ttl` are fetched again, so the ratings in them are current.
    #[must_use]
    pub fn with_track_ttl(&self, ttl: Duration) -> Self {
        let mut fresh = self.clone();
        fresh.track_ttl = Some(ttl);
        fresh
    }

    /// No waiting between requests or retries; for tests.
    #[must_use]
    pub const fn instant(mut self) -> Self {
        self.gap = Duration::ZERO;
        self.pause = Duration::ZERO;
        self
    }

    /// GET with throttling and retries. An API error other than the quota one is `{}` (nothing found), as in the Python
    /// helper; a transport failure is retried six times before it is an error.
    pub async fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value, String> {
        let params: Vec<(String, String)> = params
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect();
        let mut failure = String::new();
        for attempt in 0..6u32 {
            {
                let mut last = self.last.lock().await;
                let wait = self.gap.saturating_sub(last.elapsed());
                if !wait.is_zero() {
                    tokio::time::sleep(wait).await;
                }
                *last = Instant::now();
            }
            match self.transport.get(path, &params).await {
                Ok(answer) => {
                    if let Some(error) = answer.get("error").filter(|e| !e.is_null()) {
                        if error.get("code").and_then(Value::as_i64) == Some(4) {
                            tokio::time::sleep(self.pause.saturating_mul(3)).await;
                            continue;
                        }
                        return Ok(Value::Object(quench_serde::Map::new()));
                    }
                    return Ok(answer);
                }
                Err(e) => {
                    quench_log::warn!("deezer {path}: retry {attempt}: {e}");
                    failure = e;
                    tokio::time::sleep(self.pause.saturating_mul(attempt + 1)).await;
                }
            }
        }
        Err(format!("failed: {path}: {failure}"))
    }

    /// All pages of a list endpoint.
    pub async fn paged(&self, path: &str) -> Result<Vec<Value>, String> {
        let mut out = Vec::new();
        let mut index = 0usize;
        loop {
            let page = self
                .get(
                    path,
                    &[("limit", "100".to_string()), ("index", index.to_string())],
                )
                .await?;
            let data = page
                .get("data")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let empty = data.is_empty();
            out.extend(data);
            if page.get("next").is_none_or(Value::is_null) || empty {
                return Ok(out);
            }
            index += 100;
        }
    }

    /// The Deezer artist for a followed name: the pinned id, else the exact (normalised) name with the most fans.
    pub async fn resolve(&self, name: &str, pin: &str) -> Result<Option<ArtistHit>, String> {
        if !pin.is_empty() {
            let answer = self.get(&format!("artist/{pin}"), &[]).await?;
            return Ok(artist_hit(&answer));
        }
        let answer = self
            .get(
                "search/artist",
                &[("q", name.to_string()), ("limit", "15".to_string())],
            )
            .await?;
        let hits = answer
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(hits
            .iter()
            .filter_map(artist_hit)
            .filter(|h| norm(&h.name) == norm(name))
            .max_by_key(|h| h.fans))
    }

    /// The albums listed under an artist, oldest first (explicit before clean for the same date).
    pub async fn artist_albums(&self, artist: i64) -> Result<Vec<AlbumRef>, String> {
        let mut albums: Vec<AlbumRef> = self
            .paged(&format!("artist/{artist}/albums"))
            .await?
            .iter()
            .filter_map(|a| {
                Some(AlbumRef {
                    id: int_of(a, "id")?,
                    title: str_of(a, "title"),
                    record_type: str_of(a, "record_type"),
                    release_date: str_of(a, "release_date"),
                    explicit: a
                        .get("explicit_lyrics")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                })
            })
            .collect();
        albums.sort_by(|a, b| {
            let da = if a.release_date.is_empty() {
                "9999"
            } else {
                a.release_date.as_str()
            };
            let db = if b.release_date.is_empty() {
                "9999"
            } else {
                b.release_date.as_str()
            };
            da.cmp(db).then_with(|| (!a.explicit).cmp(&!b.explicit))
        });
        Ok(albums)
    }

    fn cached(&self, name: &str) -> Option<Value> {
        std::fs::read_to_string(self.cache.join(name))
            .ok()
            .and_then(|t| json::parse(&t).ok())
    }

    /// Is the saved file young enough to use (always, unless a track lifetime is set)?
    fn young(&self, name: &str) -> bool {
        self.track_ttl.is_none_or(|ttl| {
            std::fs::metadata(self.cache.join(name))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age < ttl)
        })
    }

    fn store(&self, name: &str, value: &Value) {
        if std::fs::create_dir_all(&self.cache).is_ok() {
            let _ = std::fs::write(self.cache.join(name), json::to_string(value));
        }
    }

    /// An album and its tracks (cached as `album_<id>.json`).
    pub async fn album(&self, id: i64) -> Result<AlbumDetail, String> {
        let file = format!("album_{id}.json");
        let detail = if let Some(v) = self.cached(&file).filter(|v| v.get("tracks").is_some()) {
            v
        } else {
            let mut d = self.get(&format!("album/{id}"), &[]).await?;
            let tracks = self.paged(&format!("album/{id}/tracks")).await?;
            if let Some(obj) = d.as_object_mut() {
                obj.insert("tracks", Value::Array(tracks));
            }
            if d.get("id").is_some() {
                self.store(&file, &d);
            }
            d
        };
        let genres = detail
            .pointer("/genres/data")
            .and_then(Value::as_array)
            .map(|g| g.iter().map(|x| str_of(x, "name")).collect())
            .unwrap_or_default();
        let tracks = detail
            .get("tracks")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|t| {
                        Some(TrackRef {
                            id: int_of(t, "id")?,
                            title: str_of(t, "title"),
                            duration: int_of(t, "duration"),
                            artist: t
                                .get("artist")
                                .and_then(|a| Some((int_of(a, "id")?, str_of(a, "name")))),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(AlbumDetail {
            info: AlbumInfo {
                deezer_id: int_of(&detail, "id").unwrap_or(id),
                title: str_of(&detail, "title"),
                release_date: str_of(&detail, "release_date"),
                record_type: str_of(&detail, "record_type"),
                genres,
            },
            tracks,
        })
    }

    /// A track with all its contributors (cached as `track_<id>.json`); `None` when Deezer does not know it. At most four
    /// of these run at once.
    pub async fn track(&self, id: i64) -> Result<Option<TrackDetail>, String> {
        let file = format!("track_{id}.json");
        // A cache entry from before `rank` was kept is fetched again.
        let value = if let Some(v) = self
            .cached(&file)
            .filter(|v| v.get("rank").is_some() && self.young(&file))
        {
            v
        } else {
            let Ok(_permit) = self.details.acquire().await else {
                return Ok(None);
            };
            let d = self.get(&format!("track/{id}"), &[]).await?;
            if int_of(&d, "id").is_none() {
                return Ok(None);
            }
            let contributors: Vec<Value> = d
                .get("contributors")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter(|c| int_of(c, "id").is_some())
                        .map(|c| {
                            let mut m = quench_serde::Map::new();
                            m.insert("id", Value::from_i64(int_of(c, "id").unwrap_or(0)));
                            m.insert("name", Value::String(str_of(c, "name")));
                            Value::Object(m)
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut m = quench_serde::Map::new();
            m.insert("id", Value::from_i64(int_of(&d, "id").unwrap_or(id)));
            m.insert("title", Value::String(str_of(&d, "title")));
            m.insert(
                "duration",
                d.get("duration").cloned().unwrap_or(Value::Null),
            );
            m.insert("isrc", Value::String(str_of(&d, "isrc")));
            m.insert("rank", d.get("rank").cloned().unwrap_or(Value::Null));
            m.insert("contributors", Value::Array(contributors));
            let out = Value::Object(m);
            self.store(&file, &out);
            out
        };
        Ok(Some(TrackDetail {
            title: str_of(&value, "title"),
            duration: int_of(&value, "duration"),
            rank: int_of(&value, "rank"),
            contributors: value
                .get("contributors")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|c| Some((int_of(c, "id")?, str_of(c, "name"))))
                        .collect()
                })
                .unwrap_or_default(),
        }))
    }

    /// An artist's best songs as Deezer lists them (`artist/{id}/top`), most popular first.
    pub async fn top_tracks(&self, artist: i64, limit: u32) -> Result<Vec<ArtistTrack>, String> {
        let answer = self
            .get(
                &format!("artist/{artist}/top"),
                &[("limit", limit.to_string())],
            )
            .await?;
        let mut tracks: Vec<ArtistTrack> = answer
            .get("data")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|t| {
                        Some(ArtistTrack {
                            id: int_of(t, "id")?,
                            title: str_of(t, "title"),
                            album: t
                                .pointer("/album/title")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            rank: int_of(t, "rank").unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        tracks.sort_by_key(|t| std::cmp::Reverse(t.rank));
        Ok(tracks)
    }

    /// The artists Deezer relates to an artist.
    pub async fn related(&self, artist: i64, limit: u32) -> Result<Vec<Related>, String> {
        let answer = self
            .get(
                &format!("artist/{artist}/related"),
                &[("limit", limit.to_string())],
            )
            .await?;
        Ok(answer
            .get("data")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|r| {
                        Some(Related {
                            id: int_of(r, "id")?,
                            name: str_of(r, "name"),
                            fans: int_of(r, "nb_fan").unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}

/// A table of canned answers keyed by path, for tests.
#[derive(Debug, Default, Clone)]
pub struct Canned(pub HashMap<String, Value>);

#[async_trait]
impl Transport for Canned {
    async fn get(&self, path: &str, params: &[(String, String)]) -> Result<Value, String> {
        let index = params
            .iter()
            .find(|(k, _)| k == "index")
            .map_or(0, |(_, v)| v.parse::<usize>().unwrap_or(0));
        if let Some(v) = self
            .0
            .get(&format!("{path}?index={index}"))
            .or_else(|| self.0.get(path))
        {
            return Ok(v.clone());
        }
        Ok(Value::Object(quench_serde::Map::new()))
    }
}
