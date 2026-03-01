use actix_web::dev::HttpServiceFactory;
use actix_web::middleware::NormalizePath;
use actix_web::{HttpResponse, Responder, delete, get, post, put, web};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;
use utoipa::OpenApi;
use zip::write::SimpleFileOptions;

#[derive(Clone, Debug)]
pub(crate) struct FileStorage {
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Serialize)]
pub(crate) struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
struct ListResponse {
    storage: String,
    path: String,
    entries: Vec<DirectoryEntry>,
}

#[derive(Debug, Serialize)]
struct StoragesResponse {
    storages: Vec<FileStorageInfo>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FileStorageInfo {
    pub name: String,
    pub root: String,
}

#[derive(Debug, Serialize)]
struct PreviewResponse {
    storage: String,
    path: String,
    kind: String,
    content: String,
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct PathQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BulkRequest {
    paths: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BulkDeleteResponse {
    deleted: usize,
}

static FILE_PREVIEW_MAX_BYTES: LazyLock<usize> = LazyLock::new(|| {
    envmnt::get_or("FILE_PREVIEW_MAX_BYTES", "65536")
        .parse()
        .unwrap_or(65536)
});

static FILE_STORAGES: LazyLock<Vec<FileStorage>> = LazyLock::new(|| {
    let raw = envmnt::get_or("FILE_STORAGES", "default=./storage/files");
    parse_file_storages(&raw)
});

fn parse_file_storages(raw: &str) -> Vec<FileStorage> {
    let mut storages = Vec::new();
    for pair in raw.split(';') {
        let trimmed = pair.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((name, path)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim();
        let path = path.trim();
        if !is_valid_storage_name(name) || path.is_empty() {
            continue;
        }
        storages.push(FileStorage {
            name: name.to_string(),
            root: PathBuf::from(path),
        });
    }
    storages
}

fn is_valid_storage_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn find_storage(name: &str) -> Option<&'static FileStorage> {
    FILE_STORAGES.iter().find(|storage| storage.name == name)
}

fn sanitize_relative_path(raw: &str) -> Option<PathBuf> {
    if raw.is_empty() {
        return Some(PathBuf::new());
    }
    let mut clean = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(clean)
}

fn join_storage_path(storage: &FileStorage, raw_path: &str) -> Option<PathBuf> {
    let relative = sanitize_relative_path(raw_path)?;
    Some(storage.root.join(relative))
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn list_storage_infos() -> Vec<FileStorageInfo> {
    FILE_STORAGES
        .iter()
        .map(|storage| FileStorageInfo {
            name: storage.name.clone(),
            root: storage.root.to_string_lossy().to_string(),
        })
        .collect()
}

pub(crate) fn list_directory(storage_name: &str, raw_path: &str) -> Option<Vec<DirectoryEntry>> {
    let storage = find_storage(storage_name)?;
    let full_path = join_storage_path(storage, raw_path)?;
    if !full_path.exists() || !full_path.is_dir() {
        return Some(Vec::new());
    }

    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(&full_path).ok()?;
    for entry in read_dir.flatten() {
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(meta) => meta,
            Err(_) => continue,
        };

        let name = entry.file_name().to_string_lossy().to_string();
        let relative = match path.strip_prefix(&storage.root) {
            Ok(value) => value,
            Err(_) => continue,
        };
        entries.push(DirectoryEntry {
            name,
            path: path_to_string(relative),
            is_dir: metadata.is_dir(),
            size_bytes: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
        });
    }

    entries.sort_by(|a, b| a.is_dir.cmp(&b.is_dir).reverse().then(a.name.cmp(&b.name)));
    Some(entries)
}

fn file_name_from_path(raw_path: &str, fallback: &str) -> String {
    Path::new(raw_path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn create_zip_from_paths(storage: &FileStorage, paths: &[String]) -> std::io::Result<Vec<u8>> {
    let buffer = Cursor::new(Vec::<u8>::new());
    let mut writer = zip::ZipWriter::new(buffer);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for raw_path in paths {
        let Some(relative) = sanitize_relative_path(raw_path) else {
            continue;
        };
        let source = storage.root.join(&relative);
        if source.is_file() {
            let mut content = std::fs::File::open(&source)?;
            writer.start_file(path_to_string(&relative), options)?;
            std::io::copy(&mut content, &mut writer)?;
            continue;
        }
        if source.is_dir() {
            add_dir_to_zip(&mut writer, &source, &storage.root, options)?;
        }
    }

    let buffer = writer.finish()?;
    Ok(buffer.into_inner())
}

fn add_dir_to_zip(
    writer: &mut zip::ZipWriter<Cursor<Vec<u8>>>,
    dir: &Path,
    root: &Path,
    options: SimpleFileOptions,
) -> std::io::Result<()> {
    let read_dir = std::fs::read_dir(dir)?;
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        let rel = match path.strip_prefix(root) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let zip_name = path_to_string(rel);

        if path.is_dir() {
            writer.add_directory(format!("{zip_name}/"), options)?;
            add_dir_to_zip(writer, &path, root, options)?;
        } else if path.is_file() {
            let mut content = std::fs::File::open(&path)?;
            writer.start_file(zip_name, options)?;
            std::io::copy(&mut content, writer)?;
        }
    }
    Ok(())
}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_storages,
        list_entries,
        upload_file,
        delete_file,
        preview_file,
        download_path,
        create_folder,
        delete_folder,
        bulk_delete,
        bulk_download,
    ),
    tags((name = "files", description = "Plain file storage endpoints"))
)]
pub struct FilesApiDoc;

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/api/v1/files")
        .wrap(NormalizePath::trim())
        .service(list_storages)
        .service(list_entries)
        .service(upload_file)
        .service(delete_file)
        .service(preview_file)
        .service(download_path)
        .service(create_folder)
        .service(delete_folder)
        .service(bulk_delete)
        .service(bulk_download)
}

#[utoipa::path(
    get,
    tags = ["files"],
    path = "/storages",
    responses((status = 200, description = "Configured storages"))
)]
#[get("/storages")]
async fn list_storages() -> impl Responder {
    HttpResponse::Ok().json(StoragesResponse {
        storages: list_storage_infos(),
    })
}

#[utoipa::path(
    get,
    tags = ["files"],
    path = "/{storage}/entries",
    params(
        ("storage" = String, Path, description = "Storage name"),
        ("path" = Option<String>, Query, description = "Path inside storage"),
    ),
    responses((status = 200, description = "Directory entries"))
)]
#[get("/{storage}/entries")]
async fn list_entries(path: web::Path<String>, query: web::Query<PathQuery>) -> impl Responder {
    let storage_name = path.into_inner();
    let raw_path = query.path.clone().unwrap_or_default();
    let Some(entries) = list_directory(&storage_name, &raw_path) else {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "storage not found"
        }));
    };

    HttpResponse::Ok().json(ListResponse {
        storage: storage_name,
        path: raw_path,
        entries,
    })
}

#[utoipa::path(
    put,
    tags = ["files"],
    path = "/{storage}/file",
    request_body = String,
    params(
        ("storage" = String, Path, description = "Storage name"),
        ("path" = String, Query, description = "Path to file"),
    ),
    responses((status = 201, description = "File uploaded"))
)]
#[put("/{storage}/file")]
async fn upload_file(
    path: web::Path<String>,
    query: web::Query<PathQuery>,
    body: web::Bytes,
) -> impl Responder {
    let storage_name = path.into_inner();
    let Some(storage) = find_storage(&storage_name) else {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "storage not found"}));
    };

    let raw_path = query.path.clone().unwrap_or_default();
    if raw_path.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "path is required"}));
    }

    let Some(file_path) = join_storage_path(storage, &raw_path) else {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "invalid path"}));
    };

    if let Some(parent) = file_path.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "failed to create parent directory"}));
    }

    match tokio::fs::write(file_path, body).await {
        Ok(_) => HttpResponse::Created().finish(),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "failed to write file"
        })),
    }
}

#[utoipa::path(
    delete,
    tags = ["files"],
    path = "/{storage}/file",
    params(
        ("storage" = String, Path, description = "Storage name"),
        ("path" = String, Query, description = "Path to file"),
    ),
    responses((status = 204, description = "File deleted"))
)]
#[delete("/{storage}/file")]
async fn delete_file(path: web::Path<String>, query: web::Query<PathQuery>) -> impl Responder {
    let storage_name = path.into_inner();
    let Some(storage) = find_storage(&storage_name) else {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "storage not found"}));
    };

    let raw_path = query.path.clone().unwrap_or_default();
    let Some(file_path) = join_storage_path(storage, &raw_path) else {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "invalid path"}));
    };

    if !file_path.is_file() {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "file not found"}));
    }

    match tokio::fs::remove_file(file_path).await {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "failed to delete file"
        })),
    }
}

#[utoipa::path(
    get,
    tags = ["files"],
    path = "/{storage}/preview",
    params(
        ("storage" = String, Path, description = "Storage name"),
        ("path" = String, Query, description = "Path to file"),
    ),
    responses((status = 200, description = "Preview content"))
)]
#[get("/{storage}/preview")]
async fn preview_file(path: web::Path<String>, query: web::Query<PathQuery>) -> impl Responder {
    let storage_name = path.into_inner();
    let Some(storage) = find_storage(&storage_name) else {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "storage not found"}));
    };

    let raw_path = query.path.clone().unwrap_or_default();
    let Some(file_path) = join_storage_path(storage, &raw_path) else {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "invalid path"}));
    };

    if !file_path.is_file() {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "file not found"}));
    }

    let data = match tokio::fs::read(&file_path).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "failed to read file"}));
        }
    };

    let max = *FILE_PREVIEW_MAX_BYTES;
    let truncated = data.len() > max;
    let data = &data[..data.len().min(max)];

    let (kind, content) = match std::str::from_utf8(data) {
        Ok(text) => ("text".to_string(), text.to_string()),
        Err(_) => (
            "base64".to_string(),
            base64::engine::general_purpose::STANDARD.encode(data),
        ),
    };

    HttpResponse::Ok().json(PreviewResponse {
        storage: storage_name,
        path: raw_path,
        kind,
        content,
        truncated,
    })
}

#[utoipa::path(
    get,
    tags = ["files"],
    path = "/{storage}/download",
    params(
        ("storage" = String, Path, description = "Storage name"),
        ("path" = String, Query, description = "Path to file or folder"),
    ),
    responses((status = 200, description = "File or zip bytes"))
)]
#[get("/{storage}/download")]
async fn download_path(path: web::Path<String>, query: web::Query<PathQuery>) -> impl Responder {
    let storage_name = path.into_inner();
    let Some(storage) = find_storage(&storage_name) else {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "storage not found"}));
    };
    let raw_path = query.path.clone().unwrap_or_default();
    let Some(full_path) = join_storage_path(storage, &raw_path) else {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "invalid path"}));
    };

    if full_path.is_file() {
        let data = match tokio::fs::read(&full_path).await {
            Ok(bytes) => bytes,
            Err(_) => {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "failed to read file"}));
            }
        };
        let file_name = file_name_from_path(&raw_path, "file.bin");
        return HttpResponse::Ok()
            .content_type("application/octet-stream")
            .append_header(("Content-Length", data.len()))
            .append_header((
                "Content-Disposition",
                format!("attachment; filename=\"{file_name}\""),
            ))
            .body(data);
    }

    if full_path.is_dir() {
        let zip_name = format!("{}.zip", file_name_from_path(&raw_path, &storage_name));
        let zip = match create_zip_from_paths(storage, std::slice::from_ref(&raw_path)) {
            Ok(bytes) => bytes,
            Err(_) => {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "failed to create zip"}));
            }
        };
        return HttpResponse::Ok()
            .content_type("application/zip")
            .append_header(("Content-Length", zip.len()))
            .append_header((
                "Content-Disposition",
                format!("attachment; filename=\"{zip_name}\""),
            ))
            .body(zip);
    }

    HttpResponse::NotFound().json(serde_json::json!({"error": "path not found"}))
}

#[utoipa::path(
    post,
    tags = ["files"],
    path = "/{storage}/folder",
    params(
        ("storage" = String, Path, description = "Storage name"),
        ("path" = String, Query, description = "Path to folder"),
    ),
    responses((status = 201, description = "Folder created"))
)]
#[post("/{storage}/folder")]
async fn create_folder(path: web::Path<String>, query: web::Query<PathQuery>) -> impl Responder {
    let storage_name = path.into_inner();
    let Some(storage) = find_storage(&storage_name) else {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "storage not found"}));
    };
    let raw_path = query.path.clone().unwrap_or_default();
    if raw_path.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "path is required"}));
    }
    let Some(folder_path) = join_storage_path(storage, &raw_path) else {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "invalid path"}));
    };

    match tokio::fs::create_dir_all(folder_path).await {
        Ok(_) => HttpResponse::Created().finish(),
        Err(_) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "failed to create folder"})),
    }
}

#[utoipa::path(
    delete,
    tags = ["files"],
    path = "/{storage}/folder",
    params(
        ("storage" = String, Path, description = "Storage name"),
        ("path" = String, Query, description = "Path to folder"),
    ),
    responses((status = 204, description = "Folder deleted"))
)]
#[delete("/{storage}/folder")]
async fn delete_folder(path: web::Path<String>, query: web::Query<PathQuery>) -> impl Responder {
    let storage_name = path.into_inner();
    let Some(storage) = find_storage(&storage_name) else {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "storage not found"}));
    };
    let raw_path = query.path.clone().unwrap_or_default();
    if raw_path.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "path is required"}));
    }
    let Some(folder_path) = join_storage_path(storage, &raw_path) else {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "invalid path"}));
    };
    if !folder_path.exists() || !folder_path.is_dir() {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "folder not found"}));
    }

    match tokio::fs::remove_dir_all(folder_path).await {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(_) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "failed to delete folder"})),
    }
}

#[utoipa::path(
    delete,
    tags = ["files"],
    path = "/{storage}/bulk",
    responses((status = 200, description = "Bulk delete result"))
)]
#[delete("/{storage}/bulk")]
async fn bulk_delete(path: web::Path<String>, body: web::Json<BulkRequest>) -> impl Responder {
    let storage_name = path.into_inner();
    let Some(storage) = find_storage(&storage_name) else {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "storage not found"}));
    };

    let mut deleted = 0usize;
    for raw_path in &body.paths {
        let Some(target) = join_storage_path(storage, raw_path) else {
            continue;
        };

        if target.is_file() {
            if tokio::fs::remove_file(&target).await.is_ok() {
                deleted += 1;
            }
            continue;
        }
        if target.is_dir() && tokio::fs::remove_dir_all(&target).await.is_ok() {
            deleted += 1;
        }
    }

    HttpResponse::Ok().json(BulkDeleteResponse { deleted })
}

#[utoipa::path(
    post,
    tags = ["files"],
    path = "/{storage}/bulk-download",
    responses((status = 200, description = "Zip archive with requested paths"))
)]
#[post("/{storage}/bulk-download")]
async fn bulk_download(path: web::Path<String>, body: web::Json<BulkRequest>) -> impl Responder {
    let storage_name = path.into_inner();
    let Some(storage) = find_storage(&storage_name) else {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "storage not found"}));
    };

    let zip = match create_zip_from_paths(storage, &body.paths) {
        Ok(bytes) => bytes,
        Err(_) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "failed to create zip"}));
        }
    };

    HttpResponse::Ok()
        .content_type("application/zip")
        .append_header(("Content-Length", zip.len()))
        .append_header((
            "Content-Disposition",
            format!("attachment; filename=\"{}-bulk.zip\"", storage_name),
        ))
        .body(zip)
}
