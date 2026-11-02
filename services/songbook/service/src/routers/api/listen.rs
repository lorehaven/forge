//! The listener API: `/api/v1/library/...`, `/api/v1/stream/{id}` and `/api/v1/playlists/...`. See `songbook_core::listen`
//! for the shapes. Open to any grant of `songbook` (see `gate`); playlists belong to whoever is signed in.

use crate::app::App;
use crate::playlists::Refused;
use crate::routers::api::{json_error, ok};
use quench_auth::domain::jwt::Claims;
use quench_core::async_trait;
use quench_http::prelude::{
    FromRequest, HttpError, Inject, Json, Path, Query, Request, Response, delete, get,
    http::StatusCode, post, put,
};
use quench_http::stream::reader_stream;
use songbook_core::{
    AlbumQuery, PlayedBody, PlaylistBody, RecentQuery, SearchQuery, SongIdsBody, SongQuery,
};
use std::io::SeekFrom;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

fn server_error(e: impl std::fmt::Display) -> Response {
    quench_log::warn!("listen api: {e}");
    json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
}

/// The person the request is for: the playlists are theirs.
#[derive(Debug)]
pub struct Me(pub String);

#[async_trait]
impl FromRequest for Me {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self(
            req.extensions()
                .get::<Claims>()
                .map_or_else(|| "dev".to_string(), |c| c.sub.clone()),
        ))
    }
}

/// The `Range` header, if there is one.
#[derive(Debug)]
pub struct RangeHeader(pub Option<String>);

#[async_trait]
impl FromRequest for RangeHeader {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self(req.header("range").map(str::to_string)))
    }
}

macro_rules! catalog {
    ($app:expr) => {
        match $app
            .library
            .catalog(&$app.index, $app.config.data.clone())
            .await
        {
            Ok(c) => c,
            Err(e) => return server_error(e),
        }
    };
}

#[get("/api/v1/library/overview")]
pub async fn overview(Inject(app): Inject<App>) -> Response {
    ok(&catalog!(app).overview())
}

/// What to listen to. The picks that are not the plain best change at midnight (UTC).
#[get("/api/v1/library/home")]
pub async fn home(Inject(app): Inject<App>) -> Response {
    let day = u64::try_from(quench_core::time::Time::now().timestamp() / 86_400).unwrap_or(0);
    ok(&catalog!(app).home(day))
}

#[get("/api/v1/library/albums")]
pub async fn albums(Inject(app): Inject<App>, Query(q): Query<AlbumQuery>) -> Response {
    ok(&catalog!(app).albums(&q))
}

#[get("/api/v1/library/songs")]
pub async fn songs(Inject(app): Inject<App>, Me(me): Me, Query(q): Query<SongQuery>) -> Response {
    let catalog = catalog!(app);
    ok(&catalog.songs(&with_playlist(&app, &me, q)))
}

/// What this person played last (each song once, newest first): the same on every device they use.
#[get("/api/v1/library/recent")]
pub async fn recent(
    Inject(app): Inject<App>,
    Me(me): Me,
    Query(q): Query<RecentQuery>,
) -> Response {
    let catalog = catalog!(app);
    let limit = if q.limit == 0 {
        40
    } else {
        (q.limit as usize).min(200)
    };
    let ids = match app.playlists.recent(&me, limit * 2) {
        Ok(ids) => ids,
        Err(e) => return server_error(e),
    };
    // Songs that have no file (any more) are left out.
    let list = catalog.songs(&SongQuery {
        ids: join(&ids),
        limit: u32::try_from(limit).unwrap_or(40),
        ..SongQuery::default()
    });
    ok(&list)
}

/// A song was started: remembered for this person.
#[post("/api/v1/history")]
pub async fn played(
    Inject(app): Inject<App>,
    Me(me): Me,
    Json(body): Json<PlayedBody>,
) -> Response {
    let now = quench_core::time::Time::now().timestamp();
    match app.playlists.played(&me, body.song_id, now) {
        Ok(()) => ok(&quench_serde::json!({})),
        Err(e) => server_error(e),
    }
}

#[get("/api/v1/library/ids")]
pub async fn ids(Inject(app): Inject<App>, Me(me): Me, Query(q): Query<SongQuery>) -> Response {
    let catalog = catalog!(app);
    ok(&catalog.ids(&with_playlist(&app, &me, q)))
}

/// A query for a playlist becomes one for its songs, in its order (what is not playable is left out by the catalog).
fn with_playlist(app: &App, me: &str, mut q: SongQuery) -> SongQuery {
    if q.playlist != 0 && q.ids.is_empty() {
        let ids = app.playlists.songs(me, q.playlist).ok().flatten();
        // An unknown playlist is an empty list, not the whole library.
        q.ids = ids.map_or_else(|| "0".to_string(), |ids| join(&ids));
        q.playlist = 0;
    }
    q
}

fn join(ids: &[i64]) -> String {
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}

#[get("/api/v1/library/search")]
pub async fn search(Inject(app): Inject<App>, Query(q): Query<SearchQuery>) -> Response {
    ok(&catalog!(app).search(&q.q))
}

/// How much one answer carries when a client asks for "the rest" (`bytes=N-`): a browser asks again as it plays, and a
/// phone is not made to hold a whole file in flight.
const CHUNK: u64 = 4 * 1024 * 1024;

/// The audio, with `Range` so that a player can seek and start at once.
#[get("/api/v1/stream/{id}")]
pub async fn stream(
    Inject(app): Inject<App>,
    RangeHeader(range): RangeHeader,
    Path(id): Path<i64>,
) -> Response {
    let catalog = catalog!(app);
    let Some(name) = catalog.file(id) else {
        return json_error(StatusCode::NOT_FOUND, "No such song.");
    };
    let path = app.config.data.join(name);
    let Ok(mut file) = tokio::fs::File::open(&path).await else {
        return json_error(StatusCode::NOT_FOUND, "The file is gone.");
    };
    let Ok(meta) = file.metadata().await else {
        return server_error("cannot read the file");
    };
    let total = meta.len();
    let kind = content_type(name);
    if total == 0 {
        return json_error(StatusCode::NOT_FOUND, "The file is empty.");
    }
    let (status, start, end) = match range.as_deref().map(|r| parse_range(r, total)) {
        None => (StatusCode::OK, 0, total - 1),
        Some(Some((start, end))) => (StatusCode::PARTIAL_CONTENT, start, end),
        Some(None) => {
            return Response::new(StatusCode::RANGE_NOT_SATISFIABLE)
                .header("content-range", format!("bytes */{total}"));
        }
    };
    let length = end - start + 1;
    if start > 0 && file.seek(SeekFrom::Start(start)).await.is_err() {
        return server_error("cannot seek in the file");
    }
    let mut response = Response::streaming(status, reader_stream(file.take(length)))
        .header("content-type", kind)
        .header("content-length", length.to_string())
        .header("accept-ranges", "bytes")
        .header("cache-control", "private, max-age=86400");
    if status == StatusCode::PARTIAL_CONTENT {
        response = response.header("content-range", format!("bytes {start}-{end}/{total}"));
    }
    response
}

/// `bytes=a-b`, `bytes=a-` (the rest, in a chunk) or `bytes=-n` (the last `n`), inside a file of `total` bytes.
#[must_use]
pub fn parse_range(header: &str, total: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?;
    let (from, to) = spec.split_once('-')?;
    if total == 0 {
        return None;
    }
    let (start, end) = if from.is_empty() {
        let n: u64 = to.parse().ok().filter(|n| *n > 0)?;
        (total.saturating_sub(n), total - 1)
    } else {
        let start: u64 = from.parse().ok()?;
        let end = if to.is_empty() {
            start.saturating_add(CHUNK - 1)
        } else {
            to.parse().ok()?
        };
        (start, end.min(total - 1))
    };
    (start <= end && start < total).then_some((start, end))
}

/// The media type a browser needs to play a file by its extension.
#[must_use]
pub fn content_type(name: &str) -> &'static str {
    match name
        .rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("m4a" | "mp4" | "aac") => "audio/mp4",
        Some("ogg" | "oga" | "opus") => "audio/ogg",
        Some("flac") => "audio/flac",
        Some("wav") => "audio/wav",
        Some("webm") => "audio/webm",
        _ => "application/octet-stream",
    }
}

fn refused(r: &Refused) -> Response {
    match r {
        Refused::Unknown => json_error(StatusCode::NOT_FOUND, "No such playlist."),
        Refused::Limit(why) => json_error(StatusCode::BAD_REQUEST, why),
    }
}

#[get("/api/v1/playlists")]
pub async fn playlists(Inject(app): Inject<App>, Me(me): Me) -> Response {
    match app.playlists.list(&me) {
        Ok(list) => ok(&list),
        Err(e) => server_error(e),
    }
}

#[post("/api/v1/playlists")]
pub async fn create_playlist(
    Inject(app): Inject<App>,
    Me(me): Me,
    Json(body): Json<PlaylistBody>,
) -> Response {
    let now = quench_core::time::Time::now().timestamp();
    match app.playlists.create(&me, &body.name, now) {
        Ok(Ok(info)) => ok(&info),
        Ok(Err(r)) => refused(&r),
        Err(e) => server_error(e),
    }
}

#[put("/api/v1/playlists/{id}")]
pub async fn rename_playlist(
    Inject(app): Inject<App>,
    Me(me): Me,
    Path(id): Path<i64>,
    Json(body): Json<PlaylistBody>,
) -> Response {
    match app.playlists.rename(&me, id, &body.name) {
        Ok(Ok(info)) => ok(&info),
        Ok(Err(r)) => refused(&r),
        Err(e) => server_error(e),
    }
}

#[delete("/api/v1/playlists/{id}")]
pub async fn delete_playlist(
    Inject(app): Inject<App>,
    Me(me): Me,
    Path(id): Path<i64>,
) -> Response {
    match app.playlists.delete(&me, id) {
        Ok(true) => ok(&quench_serde::json!({})),
        Ok(false) => refused(&Refused::Unknown),
        Err(e) => server_error(e),
    }
}

#[post("/api/v1/playlists/{id}/songs")]
pub async fn add_to_playlist(
    Inject(app): Inject<App>,
    Me(me): Me,
    Path(id): Path<i64>,
    Json(body): Json<SongIdsBody>,
) -> Response {
    match app.playlists.add(&me, id, &body.song_ids) {
        Ok(Ok(info)) => ok(&info),
        Ok(Err(r)) => refused(&r),
        Err(e) => server_error(e),
    }
}

#[put("/api/v1/playlists/{id}/songs")]
pub async fn set_playlist(
    Inject(app): Inject<App>,
    Me(me): Me,
    Path(id): Path<i64>,
    Json(body): Json<SongIdsBody>,
) -> Response {
    match app.playlists.set(&me, id, &body.song_ids) {
        Ok(Ok(info)) => ok(&info),
        Ok(Err(r)) => refused(&r),
        Err(e) => server_error(e),
    }
}

pub fn register_routes() {
    let _ = overview as fn(_) -> _;
    let _ = home as fn(_) -> _;
    let _ = albums as fn(_, _) -> _;
    let _ = songs as fn(_, _, _) -> _;
    let _ = ids as fn(_, _, _) -> _;
    let _ = recent as fn(_, _, _) -> _;
    let _ = played as fn(_, _, _) -> _;
    let _ = search as fn(_, _) -> _;
    let _ = stream as fn(_, _, _) -> _;
    let _ = playlists as fn(_, _) -> _;
    let _ = create_playlist as fn(_, _, _) -> _;
    let _ = rename_playlist as fn(_, _, _, _) -> _;
    let _ = delete_playlist as fn(_, _, _) -> _;
    let _ = add_to_playlist as fn(_, _, _, _) -> _;
    let _ = set_playlist as fn(_, _, _, _) -> _;
}
