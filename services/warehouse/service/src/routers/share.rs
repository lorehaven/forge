//! Share links, opened without signing in.
//!
//! `GET /share/{token}` serves what a person chose to share: a file as a download, or a directory as a page that lists it
//! and everything below it, directories and all. `?path=` names something below a shared directory; a shared file has no
//! `path`. The token is the only credential; a token nobody made, a share that was withdrawn and a storage that no longer
//! exists all answer the same plain 404, so the answer says nothing about which of them it was.
//!
//! Nothing here trusts the path: it is checked as every other path in this service is (no `..`, nothing absolute), and for a
//! fixed storage the result is also resolved through symlinks and must stay inside the shared directory, not merely inside
//! the storage. Files go out as downloads; `inline=1` shows a few kinds in the browser (images, audio, video, PDF, plain
//! text) and never anything that could run script on this origin.

use crate::domain::share::{self, Share};
use crate::domain::{storage, storage_file};
use crate::routers::files::ops::download::{content_type_for, stream_file};
use crate::routers::files::{Storage, confined, dynamic, relative};
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode};
use quench_serde::Deserialize;
use std::fmt::Write as _;
use std::path::{Path as FsPath, PathBuf};

/// How many names one page of a listing shows; a directory with more says so.
const LISTING_CAP: usize = 5000;

#[derive(Debug, Deserialize)]
pub struct ShareQuery {
    /// Below the shared directory; empty is the share itself.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub inline: Option<String>,
}

/// One name in a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// A caller's `?path=` as a plain relative path with `/` separators, or `None` if it is not one. Empty is allowed: the share
/// itself.
#[must_use]
pub fn clean_path(path: &str) -> Option<String> {
    if path.trim().is_empty() {
        return Some(String::new());
    }
    let relative = relative(path).ok()?;
    let parts: Option<Vec<&str>> = relative.iter().map(|part| part.to_str()).collect();
    Some(parts?.join("/"))
}

/// `a` and `b` as one storage-relative path; either may be empty.
#[must_use]
pub fn join(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("{}/{}", a.trim_end_matches('/'), b),
    }
}

/// What is at `sub` below the shared directory of a fixed storage.
#[derive(Debug, PartialEq, Eq)]
pub enum Found {
    File(PathBuf),
    Dir(PathBuf),
    Missing,
}

/// Looks `sub` up below `share_root` on disk.
///
/// `storage_root` bounds the share itself; `share_root` bounds everything below it, after symlinks are followed, so a link
/// inside the shared directory cannot lead out of it.
pub async fn resolve_static(storage_root: &FsPath, share_root: &FsPath, sub: &str) -> Found {
    if !confined(storage_root, share_root).await {
        return Found::Missing;
    }
    let target = if sub.is_empty() {
        share_root.to_path_buf()
    } else {
        let Ok(relative) = relative(sub) else {
            return Found::Missing;
        };
        share_root.join(relative)
    };
    if !confined(share_root, &target).await {
        return Found::Missing;
    }
    match tokio::fs::metadata(&target).await {
        Ok(metadata) if metadata.is_file() => Found::File(target),
        Ok(metadata) if metadata.is_dir() => Found::Dir(target),
        _ => Found::Missing,
    }
}

/// The directly contained names of `dir`, directories first, skipping staging files and anything a link leads out of
/// `share_root` to.
pub async fn static_entries(share_root: &FsPath, dir: &FsPath) -> (Vec<Entry>, bool) {
    let Ok(mut reader) = tokio::fs::read_dir(dir).await else {
        return (Vec::new(), false);
    };
    let mut entries = Vec::new();
    let mut truncated = false;
    while let Ok(Some(entry)) = reader.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        // What an interrupted upload leaves behind is not content.
        if name.starts_with('.')
            && FsPath::new(&name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("part"))
        {
            continue;
        }
        if !confined(share_root, &entry.path()).await {
            continue;
        }
        let Ok(metadata) = tokio::fs::metadata(entry.path()).await else {
            continue;
        };
        let (is_dir, size) = if metadata.is_file() {
            (false, metadata.len())
        } else if metadata.is_dir() {
            (true, 0)
        } else {
            continue;
        };
        if entries.len() >= LISTING_CAP {
            truncated = true;
            break;
        }
        entries.push(Entry { name, is_dir, size });
    }
    sort(&mut entries);
    (entries, truncated)
}

/// Directories first, then by name without regard to case.
pub fn sort(entries: &mut [Entry]) {
    entries.sort_by_key(|entry| (!entry.is_dir, entry.name.to_lowercase()));
}

/// Text made safe to put in HTML, attribute values included.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn encode(text: &str) -> String {
    quench_serde::urlencoded::percent_encode(text)
}

/// `1.5 MB`.
#[must_use]
pub fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    #[allow(clippy::cast_precision_loss, reason = "a size shown to one decimal")]
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Whether a file of this name may be shown in the browser rather than saved: the kinds that cannot run script.
#[must_use]
pub fn inline_safe(path: &str) -> bool {
    let kind = content_type_for(path);
    kind == "application/pdf"
        || kind == "text/plain"
        || (kind.starts_with("image/") && kind != "image/svg+xml")
        || kind.starts_with("video/")
        || kind.starts_with("audio/")
}

/// The page that lists a shared directory. `sub` is where in the share we are; `entries` are its names.
#[must_use]
pub fn render_listing(title: &str, sub: &str, entries: &[Entry], truncated: bool) -> String {
    let mut crumbs = format!(r#"<a href="?">{}</a>"#, escape(title));
    let mut so_far = String::new();
    for part in sub.split('/').filter(|part| !part.is_empty()) {
        so_far = join(&so_far, part);
        let _ = write!(
            crumbs,
            r#" / <a href="?path={}">{}</a>"#,
            encode(&so_far),
            escape(part)
        );
    }
    let mut rows = String::new();
    if !sub.is_empty() {
        let up = sub.rsplit_once('/').map_or("", |(parent, _)| parent);
        let href = if up.is_empty() {
            "?".to_string()
        } else {
            format!("?path={}", encode(up))
        };
        let _ = write!(
            rows,
            r#"<tr><td><a href="{href}">&#8593; ..</a></td><td></td><td></td></tr>"#
        );
    }
    for entry in entries {
        let path = join(sub, &entry.name);
        if entry.is_dir {
            let _ = write!(
                rows,
                r#"<tr><td><a href="?path={}">{}/</a></td><td></td><td></td></tr>"#,
                encode(&path),
                escape(&entry.name)
            );
        } else {
            let view = inline_safe(&entry.name).then(|| {
                format!(
                    r#" <a class="view" href="?path={}&amp;inline=1">view</a>"#,
                    encode(&path)
                )
            });
            let _ = write!(
                rows,
                r#"<tr><td><a href="?path={}">{}</a>{}</td><td class="size">{}</td><td></td></tr>"#,
                encode(&path),
                escape(&entry.name),
                view.unwrap_or_default(),
                size_text(entry.size)
            );
        }
    }
    if entries.is_empty() && sub.is_empty() {
        rows.push_str(r#"<tr><td colspan="3" class="empty">Nothing here.</td></tr>"#);
    }
    let more = truncated
        .then(|| format!(r#"<p class="more">Only the first {LISTING_CAP} names are listed.</p>"#));
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="referrer" content="no-referrer"><meta name="robots" content="noindex,nofollow">
<title>{title}</title>
<style>
body{{font:16px/1.5 system-ui,sans-serif;margin:0;background:#fafafa;color:#1a1a1a}}
main{{max-width:52rem;margin:2rem auto;padding:0 1rem}}
h1{{font-size:1.1rem;font-weight:600;margin:0 0 1rem;word-break:break-word}}
table{{width:100%;border-collapse:collapse;background:#fff;border:1px solid #ddd;border-radius:8px}}
td{{padding:.5rem .75rem;border-top:1px solid #eee;word-break:break-word}}
tr:first-child td{{border-top:0}}
.size{{text-align:right;white-space:nowrap;color:#666;width:6rem}}
.view{{font-size:.8rem;margin-left:.5rem}}
.empty,.more{{color:#666}}
a{{color:#0b5fd6;text-decoration:none}}a:hover{{text-decoration:underline}}
@media (prefers-color-scheme:dark){{body{{background:#161616;color:#e8e8e8}}table{{background:#1f1f1f;border-color:#333}}td{{border-color:#2a2a2a}}.size,.empty,.more{{color:#9a9a9a}}a{{color:#6aa9ff}}}}
</style></head>
<body><main><h1>{crumbs}</h1>
<table>{rows}</table>{more}</main></body></html>"#,
        title = escape(title),
        more = more.unwrap_or_default()
    )
}

/// The one answer for everything that is not there.
fn gone() -> Response {
    Response::text(
        StatusCode::NOT_FOUND,
        "This link does not exist or was withdrawn.",
    )
    .header("cache-control", "no-store")
    .header("x-content-type-options", "nosniff")
}

fn safe(response: Response, listing: bool) -> Response {
    let response = response
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "no-referrer")
        .header("x-robots-tag", "noindex, nofollow");
    if listing {
        response.header("cache-control", "no-store").header(
            "content-security-policy",
            "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'",
        )
    } else {
        // Whatever the file is, it may not run here.
        response
            .header(
                "content-security-policy",
                "sandbox; default-src 'none'; style-src 'unsafe-inline'",
            )
            .header("cache-control", "private, max-age=0")
    }
}

fn listing_page(title: &str, sub: &str, entries: &[Entry], truncated: bool) -> Response {
    safe(
        Response::html(
            StatusCode::OK,
            render_listing(title, sub, entries, truncated),
        ),
        true,
    )
}

/// What a share is called on the page: the last part of its path, or its storage.
fn title_of(share: &Share) -> String {
    share
        .path
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(&share.storage)
        .to_string()
}

#[get("/share/{token}")]
#[quench_log::instrument(skip(db))]
pub async fn open(
    Inject(db): Inject<Db>,
    Path(token): Path<String>,
    Query(query): Query<ShareQuery>,
) -> Response {
    if !crate::routers::files_enabled() {
        return gone();
    }
    let share = match share::by_token(&db, &token).await {
        Ok(Some(share)) => share,
        Ok(None) => return gone(),
        Err(problem) => {
            quench_log::error!("share lookup failed: {problem}");
            return Response::text(StatusCode::SERVICE_UNAVAILABLE, "Try again later.");
        }
    };
    let Some(sub) = clean_path(&query.path) else {
        return gone();
    };
    // A shared file has nothing below it.
    if !share.is_dir && !sub.is_empty() {
        return gone();
    }
    let inline = query.inline.as_deref() == Some("1");

    // A name means one thing: the database storages are tried first, then the fixed ones, as everywhere else.
    if let Ok(Some(found)) = storage::read(&db, &share.storage).await {
        return serve_dynamic(&db, &share, &found.name, &sub, inline).await;
    }
    match crate::routers::files::storage(&share.storage) {
        Some(found) => serve_static(found, &share, &sub, inline).await,
        None => gone(),
    }
}

async fn serve_static(storage: &Storage, share: &Share, sub: &str, inline: bool) -> Response {
    let share_root = if share.path.is_empty() {
        storage.root.clone()
    } else {
        let Ok(relative) = relative(&share.path) else {
            return gone();
        };
        storage.root.join(relative)
    };
    match resolve_static(&storage.root, &share_root, sub).await {
        Found::File(file) => {
            let name = join(&share.path, sub);
            let inline = inline && inline_safe(&name);
            safe(stream_file(&file, &name, inline).await, false)
        }
        // A shared file is not a directory, whatever is there now.
        Found::Dir(_) if !share.is_dir => gone(),
        Found::Dir(dir) => {
            let (entries, truncated) = static_entries(&share_root, &dir).await;
            listing_page(&title_of(share), sub, &entries, truncated)
        }
        Found::Missing => gone(),
    }
}

async fn serve_dynamic(db: &Db, share: &Share, storage: &str, sub: &str, inline: bool) -> Response {
    let key = join(&share.path, sub);
    let Some(root) = dynamic::root() else {
        return Response::text(StatusCode::SERVICE_UNAVAILABLE, "Try again later.");
    };
    // A file at exactly this path wins; otherwise the path is a directory if anything lies below it.
    if !key.is_empty() {
        match storage_file::read_file(db, storage, &key).await {
            Ok(Some((sha256, _size))) => {
                let target = dynamic::blob_path(&root, &sha256);
                let inline = inline && inline_safe(&key);
                return safe(stream_file(&target, &key, inline).await, false);
            }
            Ok(None) => {}
            Err(problem) => {
                quench_log::error!("share file lookup failed: {problem}");
                return Response::text(StatusCode::SERVICE_UNAVAILABLE, "Try again later.");
            }
        }
    }
    if !share.is_dir {
        return gone();
    }
    let limit = i64::try_from(LISTING_CAP + 1).unwrap_or(i64::MAX);
    match storage_file::children(db, storage, &key, limit).await {
        Ok(children) => {
            // Nothing below a path that is not the share's own top: it is not a directory.
            if children.is_empty() && !sub.is_empty() {
                return gone();
            }
            let truncated = children.len() > LISTING_CAP;
            let mut entries: Vec<Entry> = children
                .into_iter()
                .take(LISTING_CAP)
                .map(|child| Entry {
                    name: child.name,
                    is_dir: child.is_dir,
                    size: u64::try_from(child.size).unwrap_or(0),
                })
                .collect();
            sort(&mut entries);
            listing_page(&title_of(share), sub, &entries, truncated)
        }
        Err(problem) => {
            quench_log::error!("share listing failed: {problem}");
            Response::text(StatusCode::SERVICE_UNAVAILABLE, "Try again later.")
        }
    }
}

pub fn register_routes() {
    let _ = open as fn(_, _, _) -> _;
}
