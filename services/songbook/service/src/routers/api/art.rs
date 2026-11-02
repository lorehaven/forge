//! The pictures: `/api/v1/art/album/{id}`, `/api/v1/art/song/{id}` and `/api/v1/art/artist?name=`, each with `w=` for a
//! smaller one. The files are what the `images` job saved; a song has its album's cover (a single's own cover for a single),
//! else its artist's picture. Anything with no picture answers `404`, and the player draws a tile instead.

use crate::app::App;
use crate::images;
use crate::routers::api::json_error;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode};
use quench_serde::Deserialize;
use std::path::{Path as FsPath, PathBuf};

/// How long a device may keep a picture without asking: a picture of a song hardly ever changes, and when it does the
/// next look after this finds the new one (its `ETag` differs).
const CACHE: &str = "private, max-age=604800, stale-while-revalidate=86400";

/// The widths a picture is made in (a request for another is rounded up to one of them); the saved file is 500 wide.
pub const WIDTHS: [u32; 3] = [64, 160, 320];

#[derive(Debug, Default, Deserialize)]
pub struct ArtQuery {
    /// The width wanted (0: the saved picture as it is).
    #[serde(default)]
    pub w: u32,
    /// `artist` only: the artist's name.
    #[serde(default)]
    pub name: String,
}

/// The width to make for a request: the smallest of [`WIDTHS`] that is not smaller; `None` for the saved picture itself.
#[must_use]
pub fn width_for(wanted: u32) -> Option<u32> {
    if wanted == 0 {
        return None;
    }
    WIDTHS.iter().copied().find(|w| *w >= wanted)
}

/// The `If-None-Match` header, if there is one.
#[derive(Debug)]
pub struct IfNoneMatch(pub Option<String>);

#[quench_core::async_trait]
impl quench_http::prelude::FromRequest for IfNoneMatch {
    async fn from_request(
        req: &mut quench_http::prelude::Request,
    ) -> Result<Self, quench_http::prelude::HttpError> {
        Ok(Self(req.header("if-none-match").map(str::to_string)))
    }
}

/// Names a picture as it is now: the same text while the file and the width are, so a device can ask "is it still this?".
#[must_use]
pub fn etag(len: u64, modified_secs: u64, width: Option<u32>) -> String {
    format!("\"{len:x}-{modified_secs:x}-{}\"", width.unwrap_or(0))
}

/// The picture a file holds, by its first bytes.
#[must_use]
pub fn content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else {
        "image/jpeg"
    }
}

/// A smaller copy of `file` (made by ffmpeg, once, then kept); `file` itself when it cannot be made.
async fn small(dir: &FsPath, file: &str, width: u32) -> PathBuf {
    let source = dir.join(file);
    let target_dir = dir.join("thumbs").join(width.to_string());
    let target = target_dir.join(file);
    if target.exists() {
        return target;
    }
    if std::fs::create_dir_all(&target_dir).is_err() {
        return source;
    }
    let temp = target_dir.join(format!("{file}.tmp.jpg"));
    let made = tokio::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&source)
        .args(["-vf", &format!("scale={width}:-2"), "-q:v", "4"])
        .arg(&temp)
        .output()
        .await
        .is_ok_and(|o| o.status.success());
    if made && std::fs::rename(&temp, &target).is_ok() {
        target
    } else {
        let _ = std::fs::remove_file(&temp);
        source
    }
}

async fn serve(app: &App, kind: &str, key: &str, wanted: u32, seen: Option<String>) -> Response {
    let catalog = match app
        .library
        .catalog(&app.index, app.config.data.clone())
        .await
    {
        Ok(c) => c,
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let dir = images::dir(&app.config);
    let Some(file) = catalog
        .art(kind, key)
        .into_iter()
        .find(|name| dir.join(name).is_file())
    else {
        return json_error(StatusCode::NOT_FOUND, "No picture.");
    };
    let path = match width_for(wanted) {
        Some(width) => small(&dir, &file, width).await,
        None => dir.join(&file),
    };
    let tag = std::fs::metadata(&path).ok().map(|m| {
        let modified = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        etag(m.len(), modified, width_for(wanted))
    });
    if tag.is_some() && tag == seen {
        return Response::new(StatusCode::NOT_MODIFIED)
            .header("etag", tag.unwrap_or_default())
            .header("cache-control", CACHE);
    }
    tokio::fs::read(&path).await.map_or_else(
        |_| json_error(StatusCode::NOT_FOUND, "No picture."),
        |bytes| {
            let kind = content_type(&bytes);
            Response::from_bytes(StatusCode::OK, bytes.into())
                .header("content-type", kind)
                .header("etag", tag.unwrap_or_default())
                .header("cache-control", CACHE)
        },
    )
}

#[get("/api/v1/art/album/{id}")]
pub async fn album(
    Inject(app): Inject<App>,
    IfNoneMatch(seen): IfNoneMatch,
    Path(id): Path<i64>,
    Query(q): Query<ArtQuery>,
) -> Response {
    serve(&app, "album", &id.to_string(), q.w, seen).await
}

#[get("/api/v1/art/song/{id}")]
pub async fn song(
    Inject(app): Inject<App>,
    IfNoneMatch(seen): IfNoneMatch,
    Path(id): Path<i64>,
    Query(q): Query<ArtQuery>,
) -> Response {
    serve(&app, "song", &id.to_string(), q.w, seen).await
}

#[get("/api/v1/art/artist")]
pub async fn artist(
    Inject(app): Inject<App>,
    IfNoneMatch(seen): IfNoneMatch,
    Query(q): Query<ArtQuery>,
) -> Response {
    serve(&app, "artist", &q.name, q.w, seen).await
}

pub fn register_routes() {
    let _ = album as fn(_, _, _, _) -> _;
    let _ = song as fn(_, _, _, _) -> _;
    let _ = artist as fn(_, _, _) -> _;
}
