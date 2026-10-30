//! File storages: the list, creating, editing and removing a dynamic one, the files of one, and removing a file.

use super::{Session, error, internal, no_content, ok};
use crate::domain::storage::{self, DynamicStorage, NewStorage as DbNewStorage, StorageUpdate};
use crate::domain::storage_file;
use crate::routers::files::authz::can_on_storage_claims;
use crate::routers::files::dynamic;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Json, Path, Query, Response, get, http::StatusCode, post};
use quench_serde::Deserialize;
use std::path::Path as FsPath;
use warehouse_core::{
    Browse, BrowseFile, FileRef, NewStorage, StaticStorage, StorageChanges, Storages,
};

/// Safety bound on how many files one listing holds, not a page size.
const TREE_FILE_CAP: usize = 50_000;

/// How many rows each [`storage_file::list_files_page`] call pulls while [`dynamic_files`] walks toward [`TREE_FILE_CAP`].
const DYNAMIC_PAGE_SIZE: i64 = 2000;

fn wire(storage: DynamicStorage) -> warehouse_core::DynamicStorage {
    warehouse_core::DynamicStorage {
        name: storage.name,
        owner: storage.owner,
        max_file_bytes: storage.max_file_bytes,
        quota_bytes: storage.quota_bytes,
        used_bytes: storage.used_bytes,
        sync_enabled: storage.sync_enabled,
        created: storage.created_at.format("%Y-%m-%d %H:%M UTC"),
    }
}

/// The configured (static) storages and the database ones.
#[get("/ui/api/files/storages")]
pub async fn storages(session: Session, Inject(db): Inject<Db>) -> Response {
    if let Err(response) = session.require() {
        return response;
    }
    let fixed = crate::routers::files::storages()
        .iter()
        .map(|s| StaticStorage {
            name: s.name.clone(),
            root: s.root.display().to_string(),
        })
        .collect();
    let dynamic = if crate::routers::files_enabled() {
        storage::list(&db).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    ok(&Storages {
        fixed,
        dynamic: dynamic.into_iter().map(wire).collect(),
    })
}

/// Parses a size the form already turned into bytes: negative is not a size.
const fn non_negative(value: Option<i64>) -> Result<Option<i64>, ()> {
    match value {
        Some(v) if v < 0 => Err(()),
        other => Ok(other),
    }
}

#[post("/ui/api/files/storages")]
pub async fn create(
    session: Session,
    Inject(db): Inject<Db>,
    Json(body): Json<NewStorage>,
) -> Response {
    if let Err(response) = session.manager() {
        return response;
    }
    if !crate::routers::files_enabled() {
        return error(StatusCode::NOT_FOUND, "api_error_files_disabled");
    }

    let name = body.name.trim().to_string();
    let owner = body.owner.trim().to_string();
    if !crate::routers::files::valid_storage_name(&name) {
        return error(StatusCode::BAD_REQUEST, "api_error_invalid_storage_name");
    }
    if owner.is_empty() {
        return error(StatusCode::BAD_REQUEST, "api_error_storage_owner_required");
    }
    if crate::routers::files::storage(&name).is_some() {
        return error(StatusCode::CONFLICT, "api_error_storage_name_static_clash");
    }
    let Ok(quota) = non_negative(body.quota_bytes) else {
        return error(StatusCode::BAD_REQUEST, "api_error_invalid_quota");
    };
    let Ok(max_file_bytes) = non_negative(body.max_file_bytes) else {
        return error(StatusCode::BAD_REQUEST, "api_error_invalid_max_file");
    };

    let new = DbNewStorage {
        name,
        owner,
        max_file_bytes,
        quota_bytes: quota.unwrap_or_else(dynamic::default_quota_bytes),
        sync_enabled: body.sync_enabled,
    };
    match storage::create(&db, &new).await {
        Ok(created) => ok(&wire(created)),
        Err(problem) if problem.is_unique_violation() => {
            error(StatusCode::CONFLICT, "api_error_storage_exists")
        }
        Err(problem) if problem.is_foreign_key_violation() => {
            error(StatusCode::BAD_REQUEST, "api_error_storage_owner_unknown")
        }
        Err(problem) => {
            quench_log::error!("UI create dynamic storage failed: {problem}");
            internal()
        }
    }
}

#[post("/ui/api/files/storages/{name}/edit")]
pub async fn edit(
    session: Session,
    Path(name): Path<String>,
    Inject(db): Inject<Db>,
    Json(body): Json<StorageChanges>,
) -> Response {
    if let Err(response) = session.manager() {
        return response;
    }
    if !crate::routers::files_enabled() {
        return error(StatusCode::NOT_FOUND, "api_error_files_disabled");
    }
    let Ok(quota_bytes) = non_negative(body.quota_bytes) else {
        return error(StatusCode::BAD_REQUEST, "api_error_invalid_quota");
    };
    let Ok(max_file) = non_negative(body.max_file_bytes) else {
        return error(StatusCode::BAD_REQUEST, "api_error_invalid_max_file");
    };
    let max_file_bytes = if body.clear_max_file {
        Some(None)
    } else {
        max_file.map(Some)
    };

    let changes = StorageUpdate {
        max_file_bytes,
        quota_bytes,
        sync_enabled: Some(body.sync_enabled),
    };
    match storage::update(&db, &name, &changes).await {
        Ok(Some(updated)) => ok(&wire(updated)),
        Ok(None) => error(StatusCode::NOT_FOUND, "api_error_storage_not_found"),
        Err(problem) => {
            quench_log::error!("UI edit dynamic storage failed: {problem}");
            internal()
        }
    }
}

/// Removes a database storage with every file in it.
#[post("/ui/api/files/storages/{name}/delete")]
pub async fn delete(
    session: Session,
    Path(name): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    if let Err(response) = session.manager() {
        return response;
    }
    if !crate::routers::files_enabled() {
        return error(StatusCode::NOT_FOUND, "api_error_files_disabled");
    }
    let Ok(Some(found)) = storage::read(&db, &name).await else {
        return error(StatusCode::NOT_FOUND, "api_error_storage_not_found");
    };

    // Mirrors `routers::files::ops::storages::remove`.
    if let Some(root) = dynamic::root()
        && let Ok(files) = storage_file::list_files(&db, &found.name, "").await
    {
        for file in files {
            let _ = storage_file::delete_file(&db, &found.name, &file.path, |sha256| {
                dynamic::blob_path(&root, sha256)
            })
            .await;
        }
    }
    match storage::delete(&db, &found.name).await {
        Ok(_) => no_content(),
        Err(problem) => {
            quench_log::error!("UI delete dynamic storage failed: {problem}");
            internal()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BrowseQuery {
    pub storage: String,
}

/// Every file of a storage (up to a cap) with its size, and whether the caller may read it.
///
/// A static storage is open to any session; a database storage to its owner, a wildcard role or a
/// `warehouse:storage:<name>:read` grant.
#[get("/ui/api/files/browse")]
pub async fn browse(
    session: Session,
    Query(query): Query<BrowseQuery>,
    Inject(db): Inject<Db>,
) -> Response {
    let claims = match session.require() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let name = query.storage;

    let dynamic_storages = if crate::routers::files_enabled() {
        storage::list(&db).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let answer = |access: &str, files: Vec<BrowseFile>, truncated: bool| Browse {
        storage: name.clone(),
        access: access.to_string(),
        files,
        truncated,
    };

    if let Some(found) = dynamic_storages.iter().find(|s| s.name == name) {
        if !can_on_storage_claims(&claims, found, "read") {
            return ok(&answer(Browse::DENIED, Vec::new(), false));
        }
        let (files, truncated) = dynamic_files(&db, &name).await;
        return ok(&answer(Browse::READABLE, files, truncated));
    }
    if let Some(found) = crate::routers::files::storage(&name) {
        let (files, truncated) = static_files(&found.root).await;
        return ok(&answer(Browse::READABLE, files, truncated));
    }
    ok(&answer(Browse::MISSING, Vec::new(), false))
}

/// Walks the storage a page at a time up to [`TREE_FILE_CAP`]; one row past the cap signals `truncated`.
async fn dynamic_files(db: &Db, name: &str) -> (Vec<BrowseFile>, bool) {
    let mut files: Vec<BrowseFile> = Vec::new();
    let mut after: Option<String> = None;
    let mut truncated = false;

    loop {
        let remaining = TREE_FILE_CAP + 1 - files.len();
        let page_rows = remaining.min(usize::try_from(DYNAMIC_PAGE_SIZE).unwrap_or(usize::MAX));
        let limit = i64::try_from(page_rows).unwrap_or(i64::MAX);
        let rows = storage_file::list_files_page(db, name, "", after.as_deref(), limit, false)
            .await
            .unwrap_or_default();
        let page_len = rows.len();
        after = rows.last().map(|file| file.path.clone());
        files.extend(rows.into_iter().map(|file| BrowseFile {
            path: file.path,
            size: Some(file.size),
        }));

        if files.len() > TREE_FILE_CAP {
            files.truncate(TREE_FILE_CAP);
            truncated = true;
            break;
        }
        if page_len < page_rows {
            break;
        }
    }
    (files, truncated)
}

/// A bounded, depth-first walk of a static storage's root; `.part` staging files are skipped.
async fn static_files(root: &FsPath) -> (Vec<BrowseFile>, bool) {
    let mut out: Vec<BrowseFile> = Vec::new();
    let mut truncated = false;
    let mut stack: Vec<(std::path::PathBuf, String)> = vec![(root.to_path_buf(), String::new())];

    while let Some((dir, rel)) = stack.pop() {
        let Ok(mut reader) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = reader.next_entry().await {
            if out.len() >= TREE_FILE_CAP {
                truncated = true;
                break;
            }
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with('.')
                && FsPath::new(&file_name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("part"))
            {
                continue;
            }
            let child_rel = if rel.is_empty() {
                file_name.clone()
            } else {
                format!("{rel}/{file_name}")
            };
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            if metadata.is_dir() {
                stack.push((entry.path(), child_rel));
            } else if metadata.is_file() {
                out.push(BrowseFile {
                    path: child_rel,
                    size: Some(metadata.len().cast_signed()),
                });
            }
        }
        if out.len() >= TREE_FILE_CAP {
            truncated = true;
            break;
        }
    }
    (out, truncated)
}

/// Removes one file of a storage (a database storage keeps its blob counts and quota right; a static one is unlinked).
#[post("/ui/api/files/delete-file")]
pub async fn delete_file(
    session: Session,
    Inject(db): Inject<Db>,
    Json(body): Json<FileRef>,
) -> Response {
    if let Err(response) = session.manager() {
        return response;
    }
    if !crate::routers::files_enabled() {
        return error(StatusCode::NOT_FOUND, "api_error_files_disabled");
    }
    let (storage_name, path) = (body.storage, body.path);
    if crate::routers::files::relative(&path).is_err() {
        return error(StatusCode::BAD_REQUEST, "api_error_invalid_path");
    }

    // Dynamic storage: the domain layer owns the blob ref-count and quota.
    if storage::read(&db, &storage_name)
        .await
        .ok()
        .flatten()
        .is_some()
    {
        let Some(root) = dynamic::root() else {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "api_error_no_dynamic_root",
            );
        };
        return match storage_file::delete_file(&db, &storage_name, &path, |sha256| {
            dynamic::blob_path(&root, sha256)
        })
        .await
        {
            Ok(true) => no_content(),
            Ok(false) => error(StatusCode::NOT_FOUND, "api_error_file_not_found"),
            Err(problem) => {
                quench_log::error!("UI delete dynamic file failed: {problem}");
                internal()
            }
        };
    }

    // Static storage: a plain unlink, confined to the storage root.
    let Some(found) = crate::routers::files::storage(&storage_name) else {
        return error(StatusCode::NOT_FOUND, "api_error_storage_not_found");
    };
    let Ok(target) = crate::routers::files::resolve(found, &path) else {
        return error(StatusCode::BAD_REQUEST, "api_error_invalid_path");
    };
    if !crate::routers::files::confined(&found.root, &target).await {
        return error(StatusCode::FORBIDDEN, "api_error_path_escapes_storage");
    }
    match tokio::fs::remove_file(&target).await {
        Ok(()) => no_content(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            error(StatusCode::NOT_FOUND, "api_error_file_not_found")
        }
        Err(err) => {
            quench_log::error!("UI delete static file failed: {err}");
            internal()
        }
    }
}

pub fn register_routes() {
    let _ = storages as fn(_, _) -> _;
    let _ = create as fn(_, _, _) -> _;
    let _ = edit as fn(_, _, _, _) -> _;
    let _ = delete as fn(_, _, _) -> _;
    let _ = browse as fn(_, _, _) -> _;
    let _ = delete_file as fn(_, _, _) -> _;
}
