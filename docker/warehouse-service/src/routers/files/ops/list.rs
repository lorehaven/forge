//! `GET /api/v1/files` and `GET /api/v1/files/{storage}` - what is there.
//!
//! Listing is shallow and one directory at a time for a static storage. A
//! dynamic storage has no directory to walk - `storage_files` already holds
//! every path as a flat row, so "shallow" doesn't apply; every entry whose
//! path starts with `?prefix=` matches, regardless of depth.
//!
//! Both kinds page the same way (`?n=&last=`, see
//! `crate::routers::files::pagination`) - a dynamic storage backing a photo
//! backup client can hold tens of thousands of paths, and returning them all
//! in one response was the thing this was built to stop doing.

use super::{ResolvedStorage, authorize, error, forbidden, not_found, resolve_storage};
use crate::domain::storage_file;
use crate::routers::files::pagination::{next_link, page_size, paginate, resume_after};
use crate::routers::files::{ListQuery, PathError, confined, relative};
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench_db::prelude::Db;
use serde::Serialize;

/// The page size a caller gets when it doesn't ask for one, and the most it
/// can ask for - large enough that a mobile client browsing its own backups
/// rarely needs a second round trip, small enough that a single response
/// never again holds an entire multi-year photo library at once.
const DEFAULT_LIST_PAGE_SIZE: usize = 500;
const MAX_LIST_PAGE_SIZE: usize = 2000;

/// The path+query a `Link` header's target names, before pagination's own
/// `n`/`last` are appended by `pagination::next_link`. `prefix` and `desc`
/// have to survive onto the next page too, or a client just following
/// `Link` would silently drift back to the storage root or the default
/// (ascending) order partway through paging.
pub fn list_path(storage_name: &str, prefix: &str, desc: bool) -> String {
    let mut path = format!(
        "/api/v1/files/{}?prefix={}",
        storage_name,
        urlencoding::encode(prefix)
    );
    if desc {
        path.push_str("&desc=true");
    }
    path
}

#[derive(Serialize)]
pub struct StorageSummary {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_enabled: Option<bool>,
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

/// The storages this deployment serves - static ones by name only, since a
/// caller addresses one by name and the host's layout is not theirs to know;
/// dynamic ones with the fields a caller managing its own backup space needs
/// (`owner`, `quota_bytes`, `used_bytes`, `sync_enabled`), filtered to the
/// ones the caller may see at all (its owner, a wildcard role, or an
/// explicit grant).
#[get("")]
#[tracing::instrument(skip(request))]
pub async fn storages(request: HttpRequest, db: web::Data<Db>) -> impl Responder {
    if !crate::routers::files_enabled() {
        return not_found("file storage is not enabled");
    }

    let mut summaries: Vec<StorageSummary> = crate::routers::files::storages()
        .iter()
        .map(|storage| StorageSummary {
            name: storage.name.clone(),
            owner: None,
            quota_bytes: None,
            used_bytes: None,
            sync_enabled: None,
        })
        .collect();

    if let Ok(dynamic_storages) = crate::domain::storage::list(&db).await {
        for storage in dynamic_storages {
            if !authorize(&request, &ResolvedStorage::Dynamic(storage.clone()), "read") {
                continue;
            }
            summaries.push(StorageSummary {
                name: storage.name,
                owner: Some(storage.owner),
                quota_bytes: Some(storage.quota_bytes),
                used_bytes: Some(storage.used_bytes),
                sync_enabled: Some(storage.sync_enabled),
            });
        }
    }

    HttpResponse::Ok().json(summaries)
}

#[get("/{storage}")]
#[tracing::instrument(skip(request))]
pub async fn entries(
    request: HttpRequest,
    db: web::Data<Db>,
    storage: web::Path<String>,
    query: web::Query<ListQuery>,
) -> impl Responder {
    let storage_name = storage.into_inner();
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return *response,
    };

    if !authorize(&request, &resolved, "read") {
        return forbidden("read access to this storage is required");
    }

    match resolved {
        ResolvedStorage::Static(storage) => static_entries(storage, &query).await,
        ResolvedStorage::Dynamic(storage) => dynamic_entries(&db, &storage, &query).await,
    }
}

async fn dynamic_entries(
    db: &Db,
    storage: &crate::domain::storage::DynamicStorage,
    query: &ListQuery,
) -> HttpResponse {
    let prefix = query.prefix.clone().unwrap_or_default();
    let limit = page_size(query.n, DEFAULT_LIST_PAGE_SIZE, MAX_LIST_PAGE_SIZE);

    // One extra row, discarded by `paginate` below - it exists only to answer
    // `has_more` without a second (`COUNT`) query.
    let files = match storage_file::list_files_page(
        db,
        &storage.name,
        &prefix,
        query.last.as_deref(),
        limit as i64 + 1,
        query.desc,
    )
    .await
    {
        Ok(files) => files,
        Err(problem) => {
            tracing::error!("dynamic list failed: {problem}");
            return error(StatusCode::INTERNAL_SERVER_ERROR, "listing failed");
        }
    };

    let page = paginate(files, limit);

    // Not `entries`: actix's `#[get(...)]` on the `entries` handler below
    // already generates a unit struct of that name in this module's scope.
    let entry_list: Vec<Entry> = page
        .items
        .into_iter()
        .map(|file| {
            let name = file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&file.path)
                .to_string();
            Entry {
                name,
                path: file.path,
                kind: "file",
                size: Some(file.size as u64),
            }
        })
        .collect();

    let mut response = HttpResponse::Ok();
    if page.has_more
        && let Some(last) = entry_list.last()
    {
        response.append_header((
            "Link",
            next_link(
                &list_path(&storage.name, &prefix, query.desc),
                limit,
                &last.path,
            ),
        ));
    }

    response.json(Listing {
        storage: storage.name.clone(),
        prefix,
        entries: entry_list,
    })
}

async fn static_entries(
    storage: &'static crate::routers::files::Storage,
    query: &ListQuery,
) -> HttpResponse {
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

    let mut entry_list = Vec::new();

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

        entry_list.push(Entry {
            name,
            path,
            kind,
            size,
        });
    }

    // Directories first regardless of `desc` - a file-manager convention, not
    // part of "newest first" - then by name, reversed under `desc`. A stable
    // order either way, so a client diffing two listings sees changes rather
    // than reshuffling, and the one this storage kind's pagination resumes
    // against below.
    entry_list.sort_by(|left, right| {
        let name_order = if query.desc {
            right.name.cmp(&left.name)
        } else {
            left.name.cmp(&right.name)
        };
        (left.kind == "file")
            .cmp(&(right.kind == "file"))
            .then_with(|| name_order)
    });

    let limit = page_size(query.n, DEFAULT_LIST_PAGE_SIZE, MAX_LIST_PAGE_SIZE);
    let skip = resume_after(&entry_list, query.last.as_deref(), |entry| &entry.name);
    let page = paginate(entry_list.split_off(skip), limit);

    let mut response = HttpResponse::Ok();
    if page.has_more
        && let Some(last) = page.items.last()
    {
        response.append_header((
            "Link",
            next_link(
                &list_path(&storage.name, &prefix, query.desc),
                limit,
                &last.name,
            ),
        ));
    }

    response.json(Listing {
        storage: storage.name.clone(),
        prefix,
        entries: page.items,
    })
}
