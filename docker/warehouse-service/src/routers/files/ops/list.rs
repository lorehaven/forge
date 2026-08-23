//! `GET /api/v1/files` and `GET /api/v1/files/{storage}` - what is there.
//!
//! Listing is shallow and one directory at a time. A recursive walk of a
//! storage holding every artifact of every run would be a slow request whose
//! cost grows with history, and the caller that wants one subtree can ask for
//! it with `?prefix=`.

use super::{error, not_found, storage_or_error};
use crate::routers::files::{ListQuery, PathError, confined, relative};
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;

#[derive(Serialize)]
pub struct StorageSummary {
    pub name: String,
}

#[derive(Serialize)]
pub struct Listing {
    pub storage: String,
    pub prefix: String,
    pub entries: Vec<Entry>,
}

#[derive(Serialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    /// `file` or `directory`. Anything else on disk is not listed at all.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// The storages this deployment serves - names only.
///
/// Deliberately not their directories: a caller addresses a storage by name,
/// and the host's layout is not theirs to know.
#[get("")]
#[tracing::instrument]
pub async fn storages() -> impl Responder {
    if !crate::routers::files_enabled() {
        return not_found("file storage is not enabled");
    }

    let storages: Vec<StorageSummary> = crate::routers::files::storages()
        .iter()
        .map(|storage| StorageSummary {
            name: storage.name.clone(),
        })
        .collect();

    HttpResponse::Ok().json(storages)
}

#[get("/{storage}")]
#[tracing::instrument]
pub async fn entries(storage: web::Path<String>, query: web::Query<ListQuery>) -> impl Responder {
    let storage_name = storage.into_inner();
    let storage = match storage_or_error(&storage_name) {
        Ok(storage) => storage,
        Err(response) => return *response,
    };

    let prefix = query.prefix.clone().unwrap_or_default();

    // An empty prefix means the storage root, which `relative` refuses as a
    // path - correct there, since you cannot upload *to* the root, but this is
    // the one caller for which it is the obvious default.
    let directory = if prefix.trim().is_empty() {
        storage.root.clone()
    } else {
        match relative(&prefix) {
            Ok(relative) => storage.root.join(relative),
            Err(why) => {
                let status = match why {
                    PathError::Empty => StatusCode::BAD_REQUEST,
                    _ => StatusCode::FORBIDDEN,
                };
                return error(status, why.message());
            }
        }
    };

    if !confined(&storage.root, &directory).await {
        return error(StatusCode::FORBIDDEN, "prefix resolves outside the storage");
    }

    let mut reader = match tokio::fs::read_dir(&directory).await {
        Ok(reader) => reader,
        Err(_) => return not_found("no such directory"),
    };

    let mut entries = Vec::new();

    while let Ok(Some(entry)) = reader.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();

        // The staging files an interrupted upload can leave behind are not
        // content, and listing them would offer callers a path that is about
        // to disappear.
        if name.starts_with('.') && name.ends_with(".part") {
            continue;
        }

        // `DirEntry::metadata` describes the link rather than what it points
        // at, which would make every symlink an "other" and drop it below. A
        // symlink that stays inside the storage serves perfectly well over
        // `GET`, so a listing that hides it disagrees with the rest of the API.
        let Ok(link_type) = entry.file_type().await else {
            continue;
        };

        if link_type.is_symlink() && !confined(&storage.root, &entry.path()).await {
            // One that leaves the storage is a different matter: `GET` answers
            // it 403, so listing it would advertise a path that cannot be
            // fetched - and name a file outside the storage while doing it.
            continue;
        }

        // Follows the link, unlike the call above.
        let Ok(metadata) = tokio::fs::metadata(entry.path()).await else {
            // A symlink to nothing lands here. Not content, and not worth a
            // row that 404s the moment anybody follows it.
            continue;
        };

        let path = if prefix.trim().is_empty() {
            name.clone()
        } else {
            format!("{}/{name}", prefix.trim_end_matches('/'))
        };

        let (kind, size) = if metadata.is_file() {
            ("file", Some(metadata.len()))
        } else if metadata.is_dir() {
            ("directory", None)
        } else {
            // A socket or device node in a storage is not something a caller
            // can do anything with, and offering its path invites a GET that
            // would block.
            continue;
        };

        entries.push(Entry {
            name,
            path,
            kind,
            size,
        });
    }

    // Directories first, then by name - a stable order, so a client diffing two
    // listings sees changes rather than reshuffling.
    entries.sort_by(|left, right| {
        (left.kind == "file")
            .cmp(&(right.kind == "file"))
            .then_with(|| left.name.cmp(&right.name))
    });

    HttpResponse::Ok().json(Listing {
        storage: storage.name.clone(),
        prefix,
        entries,
    })
}
