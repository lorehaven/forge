//! `GET /api/v1/files` and `GET /api/v1/files/{storage}` - what is there.
//! Static storages list shallowly; dynamic ones match `?prefix=` against flat rows.

use super::{ResolvedStorage, authorize, error, forbidden, not_found, resolve_storage};
use crate::domain::storage_file;
use crate::routers::files::pagination::{next_link, page_size, paginate, resume_after};
use crate::routers::files::{ListQuery, OptionalClaims, PathError, confined, relative};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode};
use serde::Serialize;

/// Default and max page size - big enough for one round trip, small enough to bound a response.
const DEFAULT_LIST_PAGE_SIZE: usize = 500;
const MAX_LIST_PAGE_SIZE: usize = 2000;

/// Base `Link` target before `pagination::next_link` appends `n`/`last`;
/// `prefix`/`desc` must survive onto the next page too.
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

#[derive(Serialize, Clone)]
pub struct Entry {
    pub name: String,
    pub path: String,
    /// `file` or `directory`. Anything else on disk is not listed at all.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// The storages this deployment serves - static ones by name only, dynamic
/// ones with owner/quota/usage fields, filtered to what the caller may see.
#[get("/api/v1/files")]
#[tracing::instrument(skip(claims, config, db))]
pub async fn storages(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
) -> Response {
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
            if !authorize(
                claims.as_ref(),
                &config,
                &ResolvedStorage::Dynamic(storage.clone()),
                "read",
            ) {
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

    Response::json(StatusCode::OK, &summaries)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[get("/api/v1/files/{storage}")]
#[tracing::instrument(skip(claims, config, db))]
pub async fn entries(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Path(storage_name): Path<String>,
    Query(query): Query<ListQuery>,
) -> Response {
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };

    if !authorize(claims.as_ref(), &config, &resolved, "read") {
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
) -> Response {
    let prefix = query.prefix.clone().unwrap_or_default();
    let limit = page_size(query.n, DEFAULT_LIST_PAGE_SIZE, MAX_LIST_PAGE_SIZE);

    // One extra row, discarded by `paginate` - answers `has_more` without a COUNT query.
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

    // Not `entries`: that name is already taken by the handler above.
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

    let mut response = Response::json(
        StatusCode::OK,
        &Listing {
            storage: storage.name.clone(),
            prefix: prefix.clone(),
            entries: entry_list.clone(),
        },
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
    if page.has_more
        && let Some(last) = entry_list.last()
    {
        response = response.header(
            "Link",
            next_link(
                &list_path(&storage.name, &prefix, query.desc),
                limit,
                &last.path,
            ),
        );
    }
    response
}

async fn static_entries(
    storage: &'static crate::routers::files::Storage,
    query: &ListQuery,
) -> Response {
    let prefix = query.prefix.clone().unwrap_or_default();

    // Empty prefix = storage root; `relative` refuses that path elsewhere, but not here.
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

        // Staging files an interrupted upload leaves behind aren't content.
        if name.starts_with('.') && name.ends_with(".part") {
            continue;
        }

        // `file_type()` sees the link, not the target, so a symlink needs its own confinement check.
        let Ok(link_type) = entry.file_type().await else {
            continue;
        };

        if link_type.is_symlink() && !confined(&storage.root, &entry.path()).await {
            continue;
        }

        // Follows the link, unlike the call above; a broken symlink lands in the Err arm.
        let Ok(metadata) = tokio::fs::metadata(entry.path()).await else {
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
            // A socket or device node - offering it would invite a blocking GET.
            continue;
        };

        entry_list.push(Entry {
            name,
            path,
            kind,
            size,
        });
    }

    // Directories first regardless of `desc` (file-manager convention), then by name.
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

    let mut response = Response::json(
        StatusCode::OK,
        &Listing {
            storage: storage.name.clone(),
            prefix: prefix.clone(),
            entries: page.items.clone(),
        },
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
    if page.has_more
        && let Some(last) = page.items.last()
    {
        response = response.header(
            "Link",
            next_link(
                &list_path(&storage.name, &prefix, query.desc),
                limit,
                &last.name,
            ),
        );
    }
    response
}

pub fn register_routes() {
    let _ = storages as fn(_, _, _) -> _;
    let _ = entries as fn(_, _, _, _, _) -> _;
}
